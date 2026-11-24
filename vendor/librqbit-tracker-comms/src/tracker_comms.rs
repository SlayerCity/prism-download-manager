use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::net::SocketAddrV4;
use std::net::SocketAddrV6;
use std::sync::Arc;
use parking_lot::RwLock;
use std::time::Duration;

use anyhow::Context;
use anyhow::bail;
use backon::ExponentialBuilder;
use backon::Retryable;
use futures::FutureExt;
use futures::StreamExt;
use futures::future::Either;
use futures::stream::BoxStream;
use futures::stream::FuturesUnordered;
use tracing::Instrument;
use tracing::debug;
use tracing::debug_span;
use tracing::trace;
use tracing::trace_span;
use url::Url;

use crate::tracker_comms_http;
use crate::tracker_comms_udp;
use crate::tracker_comms_udp::UdpTrackerClient;
use librqbit_core::hash_id::Id20;

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerAnnounceStatus {
    pub tracker: String,
    pub state: String,
    pub interval_secs: Option<u64>,
    pub seeders: Option<u64>,
    pub leechers: Option<u64>,
    pub error: Option<String>,
}

pub type TrackerStatusStore = Arc<RwLock<HashMap<String, TrackerAnnounceStatus>>>;

pub struct TrackerComms {
    info_hash: Id20,
    peer_id: Id20,
    stats: Box<dyn TorrentStatsProvider>,
    force_tracker_interval: Option<Duration>,
    tx: Sender,
    // This MUST be set as trackers don't work with 0 port.
    announce_port: u16,
    reqwest_client: reqwest::Client,
    key: u32,
    tracker_status: TrackerStatusStore,
}

#[derive(Clone, Copy, Default)]
pub enum TrackerCommsStatsState {
    #[default]
    None,
    Initializing,
    Paused,
    Live,
}

#[derive(Clone, Copy, Default)]
pub struct TrackerCommsStats {
    pub uploaded_bytes: u64,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub torrent_state: TrackerCommsStatsState,
}

impl TrackerCommsStats {
    pub fn get_left_to_download_bytes(&self) -> u64 {
        let total = self.total_bytes;
        let down = self.downloaded_bytes;
        if total >= down {
            return total - down;
        }
        0
    }

    pub fn is_completed(&self) -> bool {
        self.downloaded_bytes >= self.total_bytes
    }
}

pub trait TorrentStatsProvider: Send + Sync {
    fn get(&self) -> TrackerCommsStats;
}

impl TorrentStatsProvider for () {
    fn get(&self) -> TrackerCommsStats {
        Default::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AnnounceEvent {
    Started,
    Stopped,
    Completed,
}

#[derive(Default)]
struct AnnounceEventState {
    started: bool,
    completed: bool,
    stopped: bool,
}

impl AnnounceEventState {
    fn next(&self, stats: &TrackerCommsStats) -> Option<AnnounceEvent> {
        if !self.started {
            Some(AnnounceEvent::Started)
        } else if !self.stopped && matches!(stats.torrent_state, TrackerCommsStatsState::Paused) {
            Some(AnnounceEvent::Stopped)
        } else if !self.completed
            && matches!(stats.torrent_state, TrackerCommsStatsState::Live)
            && stats.is_completed()
        {
            Some(AnnounceEvent::Completed)
        } else {
            None
        }
    }

    fn record_success(&mut self, event: Option<AnnounceEvent>) {
        match event {
            Some(AnnounceEvent::Started) => self.started = true,
            Some(AnnounceEvent::Stopped) => self.stopped = true,
            Some(AnnounceEvent::Completed) => self.completed = true,
            None => {}
        }
    }
}

impl AnnounceEvent {
    fn as_http(self) -> tracker_comms_http::TrackerRequestEvent {
        match self {
            Self::Started => tracker_comms_http::TrackerRequestEvent::Started,
            Self::Stopped => tracker_comms_http::TrackerRequestEvent::Stopped,
            Self::Completed => tracker_comms_http::TrackerRequestEvent::Completed,
        }
    }

    fn as_udp(self) -> u32 {
        use tracker_comms_udp::{EVENT_COMPLETED, EVENT_STARTED, EVENT_STOPPED};
        match self {
            Self::Started => EVENT_STARTED,
            Self::Stopped => EVENT_STOPPED,
            Self::Completed => EVENT_COMPLETED,
        }
    }
}

type Sender = tokio::sync::mpsc::Sender<SocketAddr>;

enum SupportedTracker {
    Udp(Url),
    Http(Url),
}

impl std::fmt::Debug for SupportedTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SupportedTracker::Udp(u) => std::fmt::Display::fmt(u, f),
            SupportedTracker::Http(u) => std::fmt::Display::fmt(u, f),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum UdpTrackerResolveResult {
    One(SocketAddr),
    Two(SocketAddrV4, SocketAddrV6),
}

async fn udp_tracker_to_socket_addrs(
    host: url::Host<&str>,
    port: u16,
) -> anyhow::Result<UdpTrackerResolveResult> {
    let res = match host {
        url::Host::Domain(name) => {
            // Use the first IPv4 and the first IPv6 addresses only.

            let mut v4: Option<SocketAddrV4> = None;
            let mut v6: Option<SocketAddrV6> = None;
            for addr in tokio::net::lookup_host((name, port))
                .await
                .with_context(|| format!("error looking up hostname {name}"))?
            {
                match (v4, v6, addr) {
                    (None, _, SocketAddr::V4(addr)) => v4 = Some(addr),
                    (_, None, SocketAddr::V6(addr)) => v6 = Some(addr),
                    _ => continue,
                }
            }
            let res = match (v4, v6) {
                (Some(v4), Some(v6)) => UdpTrackerResolveResult::Two(v4, v6),
                (Some(v4), None) => UdpTrackerResolveResult::One(v4.into()),
                (None, Some(v6)) => UdpTrackerResolveResult::One(v6.into()),
                _ => anyhow::bail!("zero addresses returned looking up {name}"),
            };
            trace!(?res, "resolved");
            res
        }
        url::Host::Ipv4(addr) => UdpTrackerResolveResult::One((addr, port).into()),
        url::Host::Ipv6(addr) => UdpTrackerResolveResult::One((addr, port).into()),
    };
    Ok(res)
}

impl TrackerComms {
    // TODO: fix too many args
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        info_hash: Id20,
        peer_id: Id20,
        trackers: HashSet<Url>,
        stats: Box<dyn TorrentStatsProvider>,
        force_interval: Option<Duration>,
        announce_port: u16,
        reqwest_client: reqwest::Client,
        udp_client: UdpTrackerClient,
        tracker_status: TrackerStatusStore,
    ) -> Option<BoxStream<'static, SocketAddr>> {
        let trackers = trackers
            .into_iter()
            .filter_map(|t| match t.scheme() {
                "http" | "https" => Some(SupportedTracker::Http(t)),
                "udp" => Some(SupportedTracker::Udp(t)),
                _ => {
                    debug!("unsupported tracker URL: {}", t);
                    None
                }
            })
            .collect::<Vec<_>>();
        if trackers.is_empty() {
            debug!(?info_hash, "trackers list is empty");
            return None;
        }

        tracing::trace!(?trackers);

        let (tx, mut rx) = tokio::sync::mpsc::channel::<SocketAddr>(16);

        let s = async_stream::stream! {
            use futures::StreamExt;
            let comms = Arc::new(Self {
                info_hash,
                peer_id,
                stats,
                force_tracker_interval: force_interval,
                tx,
                announce_port,
                reqwest_client,
                key: rand::random(),
                tracker_status,
            });
            let mut futures = FuturesUnordered::new();
            for tracker in trackers {
                futures.push(comms.add_tracker(tracker, &udp_client))
            }
            while !(futures.is_empty()) {
                tokio::select! {
                    addr = rx.recv() => {
                        if let Some(addr) = addr {
                            yield addr;
                        }
                    }
                    e = futures.next(), if !futures.is_empty() => {
                        if let Some(Err(e)) = e {
                            debug!("error: {e}");
                        }
                    }
                }
            }
        };

        Some(s.boxed())
    }

    fn add_tracker(
        &self,
        url: SupportedTracker,
        client: &UdpTrackerClient,
    ) -> Either<
        impl std::future::Future<Output = anyhow::Result<()>> + '_ + Send,
        impl std::future::Future<Output = anyhow::Result<()>> + '_ + Send,
    > {
        let info_hash = self.info_hash;
        match url {
            SupportedTracker::Udp(url) => {
                let span = debug_span!(parent: None, "udp_tracker", tracker = %url, info_hash = ?info_hash);
                self.task_single_tracker_monitor_udp(url, client.clone())
                    .instrument(span)
                    .right_future()
            }
            SupportedTracker::Http(url) => {
                let span = debug_span!(
                    parent: None,
                    "http_tracker",
                    tracker = %url,
                    info_hash = ?info_hash
                );
                self.task_single_tracker_monitor_http(url)
                    .instrument(span)
                    .left_future()
            }
        }
    }

    fn tracker_label(url: &Url) -> String {
        match (url.host_str(), url.port()) {
            (Some(host), Some(port)) => format!("{host}:{port}"),
            (Some(host), None) => host.to_string(),
            _ => "Tracker".to_string(),
        }
    }

    fn set_tracker_error(&self, url: &Url, error: &str) {
        self.set_tracker_error_url(url, error);
    }

    fn safe_tracker_error(url: &Url, error: &str) -> String {
        let label = Self::tracker_label(url);
        error.replace(url.as_str(), &label).chars().take(240).collect()
    }

    fn set_tracker_error_url(&self, url: &Url, error: &str) {
        let label = Self::tracker_label(url);
        let safe_error = Self::safe_tracker_error(url, error);
        self.tracker_status.write().insert(url.to_string(), TrackerAnnounceStatus {
            tracker: label,
            state: "error".to_string(),
            interval_secs: None,
            seeders: None,
            leechers: None,
            error: Some(safe_error.chars().take(240).collect()),
        });
    }

    fn set_tracker_success(&self, url: &Url, interval: Duration, seeders: u64, leechers: u64) {
        self.set_tracker_success_counts(url, interval, Some(seeders), Some(leechers));
    }

    fn set_tracker_success_counts(
        &self,
        url: &Url,
        interval: Duration,
        seeders: Option<u64>,
        leechers: Option<u64>,
    ) {
        self.tracker_status.write().insert(url.to_string(), TrackerAnnounceStatus {
            tracker: Self::tracker_label(url),
            state: "working".to_string(),
            interval_secs: Some(interval.as_secs()),
            seeders,
            leechers,
            error: None,
        });
    }

    async fn task_single_tracker_monitor_http(&self, tracker_url: Url) -> anyhow::Result<()> {
        trace!(url=%tracker_url, "starting monitor");
        let mut events = AnnounceEventState::default();

        loop {
            let event = events.next(&self.stats.get());
            let interval = (|| self.tracker_one_request_http(&tracker_url, event))
                .retry(
                    ExponentialBuilder::new()
                        .without_max_times()
                        .with_jitter()
                        .with_factor(2.)
                        .with_min_delay(Duration::from_secs(10))
                        .with_max_delay(Duration::from_secs(600)),
                )
                .notify(|err, retry_in| {
                    self.set_tracker_error(&tracker_url, &format!("{err:#}"));
                    debug!(?retry_in, "error calling tracker: {err:#}")
                })
                .await
                .context("this shouldn't fail")?;

            events.record_success(event);
            if event == Some(AnnounceEvent::Stopped) {
                break Ok(());
            }
            let interval = self.force_tracker_interval.unwrap_or(interval);
            debug!("sleeping for {:?} after calling tracker", interval);
            tokio::time::sleep(interval).await;
        }
    }

    async fn tracker_one_request_http(
        &self,
        tracker_url: &Url,
        event: Option<AnnounceEvent>,
    ) -> anyhow::Result<Duration> {
        let stats = self.stats.get();
        let request = tracker_comms_http::TrackerRequest {
            info_hash: &self.info_hash,
            peer_id: &self.peer_id,
            port: self.announce_port,
            uploaded: stats.uploaded_bytes,
            downloaded: stats.downloaded_bytes,
            left: stats.get_left_to_download_bytes(),
            compact: true,
            no_peer_id: false,
            event: event.map(AnnounceEvent::as_http),
            ip: None,
            numwant: None,
            key: Some(self.key),
            trackerid: None,
        };

        let mut url = tracker_url.clone();

        let mut queries = request.as_querystring();
        if let Some(url_query) = url.query() {
            queries.push_str(&format!("&{}", url_query));
        }
        url.set_query(Some(&queries));

        let response: reqwest::Response = self.reqwest_client.get(url).send().await?;
        if !response.status().is_success() {
            anyhow::bail!("tracker responded with {:?}", response.status());
        }
        let bytes = response.bytes().await?;
        if let Ok((error, _)) =
            bencode::from_bytes_with_rest::<tracker_comms_http::TrackerError>(&bytes)
        {
            anyhow::bail!(
                "tracker returned failure. Failure reason: {}",
                error.failure_reason
            )
        };
        let response = bencode::from_bytes_with_rest::<tracker_comms_http::TrackerResponse>(&bytes)
            .map_err(|e| {
                tracing::trace!("error deserializing TrackerResponse: {e:#}");
                e.into_kind()
            })?
            .0;

        for peer in response.iter_peers() {
            self.tx.send(peer).await?;
        }
        let interval = response.min_interval.unwrap_or(response.interval);
        self.set_tracker_success(
            tracker_url,
            Duration::from_secs(interval),
            response.complete,
            response.incomplete,
        );
        Ok(Duration::from_secs(interval))
    }

    async fn task_single_tracker_monitor_udp(
        &self,
        url: Url,
        client: UdpTrackerClient,
    ) -> anyhow::Result<()> {
        if url.scheme() != "udp" {
            bail!("expected UDP scheme in {}", url);
        }
        let (host, port) = (
            url.host().context("missing host")?,
            url.port().context("missing port")?,
        );

        let mut sleep_interval: Option<Duration> = None;
        let mut prev_addrs: Option<UdpTrackerResolveResult> = None;
        let mut events = AnnounceEventState::default();
        loop {
            if let Some(i) = sleep_interval {
                trace!(interval=?sleep_interval, "sleeping");
                tokio::time::sleep(i).await;
            }

            // This should retry forever until the addrs are resolved.
            let addrs = (async || {
                udp_tracker_to_socket_addrs(host.clone(), port)
                    .instrument(trace_span!("resolve", ?host))
                    .await
                    .or_else(|err| prev_addrs.ok_or(err))
            })
            .retry(
                ExponentialBuilder::new()
                    .without_max_times()
                    .with_max_delay(Duration::from_secs(60))
                    .with_jitter(),
            )
            .notify(|err, retry| {
                self.set_tracker_error_url(&url, &format!("error resolving tracker: {err:#}"));
                debug!(retry_in=?retry, "error resolving tracker: {err:#}")
            })
            .await
            .context("this shouldn't happen: failed resolving tracker addrs")?;

            prev_addrs = Some(addrs);
            let stats = self.stats.get();
            let event = events.next(&stats);
            let mut announced = false;

            match addrs {
                UdpTrackerResolveResult::One(addr) => {
                    match self
                        .tracker_one_request_udp(addr, &url, &client, stats, event)
                        .instrument(trace_span!("udp request", ?addr))
                        .await
                    {
                        Ok(sleep) => {
                            sleep_interval = Some(sleep);
                            announced = true;
                        }
                        Err(error) => {
                            self.set_tracker_error_url(&url, &format!("{error:#}"));
                            sleep_interval = Some(sleep_interval.unwrap_or(Duration::from_secs(60)))
                        }
                    }
                }
                UdpTrackerResolveResult::Two(v4, v6) => {
                    let (r4, r6) = tokio::join!(
                        self.tracker_one_request_udp(v4.into(), &url, &client, stats, event)
                            .instrument(trace_span!("udp request", addr=?v4)),
                        self.tracker_one_request_udp(v6.into(), &url, &client, stats, event)
                            .instrument(trace_span!("udp request", addr=?v6))
                    );
                    announced = r4.is_ok() || r6.is_ok();
                    match (r4, r6) {
                        (Ok(sleep4), Ok(sleep6)) => {
                            self.set_tracker_success_counts(&url, sleep4.min(sleep6), None, None);
                            sleep_interval = Some(sleep4.min(sleep6));
                        }
                        (Ok(sleep), Err(error)) | (Err(error), Ok(sleep)) => {
                            self.set_tracker_success_counts(&url, sleep, None, None);
                            self.set_tracker_error_url(&url, &format!("{error:#}"));
                            sleep_interval = Some(sleep);
                        }
                        (Err(error4), Err(error6)) => {
                            self.set_tracker_error_url(&url, &format!("IPv4: {error4:#}; IPv6: {error6:#}"));
                            sleep_interval = Some(sleep_interval.unwrap_or(Duration::from_secs(60)));
                        }
                    }
                }
            }
            if announced {
                events.record_success(event);
            }
            if event == Some(AnnounceEvent::Stopped) && announced {
                break Ok(());
            }
        }
    }

    async fn tracker_one_request_udp(
        &self,
        addr: SocketAddr,
        tracker_url: &Url,
        client: &UdpTrackerClient,
        stats: TrackerCommsStats,
        event: Option<AnnounceEvent>,
    ) -> anyhow::Result<Duration> {
        use tracker_comms_udp::*;

        let request = AnnounceFields {
            info_hash: self.info_hash,
            peer_id: self.peer_id,
            downloaded: stats.downloaded_bytes,
            left: stats.get_left_to_download_bytes(),
            uploaded: stats.uploaded_bytes,
            event: event.map(AnnounceEvent::as_udp).unwrap_or(EVENT_NONE),
            key: self.key,
            port: self.announce_port,
        };

        match client.announce(addr, request).await {
            Ok(response) => {
                trace!(len = response.addrs.len(), "received announce response");
                for addr in response.addrs {
                    self.tx.send(addr).await.context("rx closed")?;
                }
                let sleep = response.interval.max(5);
                let sleep = Duration::from_secs(sleep as u64);
                self.set_tracker_success_counts(&tracker_url, sleep, Some(response.seeders as u64), Some(response.leechers as u64));
                Ok(sleep)
            }
            Err(e) => {
                debug!(?addr, "error reading announce response: {e:#}");
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod announce_event_tests {
    use super::*;

    fn stats(state: TrackerCommsStatsState, downloaded: u64, total: u64) -> TrackerCommsStats {
        TrackerCommsStats {
            downloaded_bytes: downloaded,
            total_bytes: total,
            torrent_state: state,
            ..Default::default()
        }
    }

    #[test]
    fn tracker_errors_hide_announce_url_secrets_and_are_bounded() {
        let url = Url::parse("https://tracker.example/announce?passkey=secret").unwrap();
        let error = format!("request failed for {url}");
        let safe = TrackerComms::safe_tracker_error(&url, &error);

        assert!(safe.contains("tracker.example"));
        assert!(!safe.contains("passkey"));
        assert!(!safe.contains("secret"));
        assert!(safe.len() <= 240);
    }

    #[test]
    fn announce_events_follow_started_completed_and_stopped_transitions() {
        let mut events = AnnounceEventState::default();
        let live = stats(TrackerCommsStatsState::Live, 0, 100);
        let complete = stats(TrackerCommsStatsState::Live, 100, 100);
        let paused = stats(TrackerCommsStatsState::Paused, 100, 100);

        assert_eq!(events.next(&live), Some(AnnounceEvent::Started));
        // A failed announce must not consume the event.
        assert_eq!(events.next(&live), Some(AnnounceEvent::Started));
        events.record_success(Some(AnnounceEvent::Started));
        assert_eq!(events.next(&live), None);
        assert_eq!(events.next(&complete), Some(AnnounceEvent::Completed));
        events.record_success(Some(AnnounceEvent::Completed));
        assert_eq!(events.next(&complete), None);
        assert_eq!(events.next(&paused), Some(AnnounceEvent::Stopped));
        events.record_success(Some(AnnounceEvent::Stopped));
        assert_eq!(events.next(&paused), None);
    }
}
