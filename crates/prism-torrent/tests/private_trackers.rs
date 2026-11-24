//! Private torrents must not inherit session-wide public tracker URLs.
//! This protects the central privacy boundary of #109 without contacting the public internet.

use prism_torrent::{TorrentEngine, TorrentSource};
use sha1::{Digest, Sha1};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

fn bstr(out: &mut Vec<u8>, s: &[u8]) {
    out.extend_from_slice(format!("{}:", s.len()).as_bytes());
    out.extend_from_slice(s);
}

fn bint(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(format!("i{n}e").as_bytes());
}

fn torrent(announce: &str, private: bool) -> Vec<u8> {
    let data = b"private test payload";
    let mut info = b"d".to_vec();
    bstr(&mut info, b"length");
    bint(&mut info, data.len());
    bstr(&mut info, b"name");
    bstr(&mut info, b"private-test.bin");
    bstr(&mut info, b"piece length");
    bint(&mut info, 16 * 1024);
    bstr(&mut info, b"pieces");
    bstr(&mut info, &Sha1::digest(data));
    if private {
        bstr(&mut info, b"private");
        bint(&mut info, 1);
    }
    info.push(b'e');

    let mut torrent = b"d".to_vec();
    bstr(&mut torrent, b"announce");
    bstr(&mut torrent, announce.as_bytes());
    bstr(&mut torrent, b"info");
    torrent.extend_from_slice(&info);
    torrent.push(b'e');
    torrent
}

#[derive(Default)]
struct TrackerRequests {
    requests: Vec<String>,
    user_agents: Vec<String>,
}

async fn tracker() -> (String, Arc<Mutex<TrackerRequests>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(TrackerRequests::default()));
    let captured = requests.clone();
    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let mut reader = BufReader::new(stream);
            let mut request_line = String::new();
            let mut user_agent = String::new();
            if reader.read_line(&mut request_line).await.is_ok() {
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).await.is_err() || header == "\r\n" {
                        break;
                    }
                    if let Some((name, value)) = header.split_once(':')
                        && name.eq_ignore_ascii_case("user-agent")
                    {
                        user_agent = value.trim().to_owned();
                    }
                }
                let mut captured = captured.lock().unwrap();
                captured.requests.push(request_line.trim().to_owned());
                captured.user_agents.push(user_agent);
            }
            let body = b"d8:intervali30e5:peers0:e";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            if reader
                .get_mut()
                .write_all(response.as_bytes())
                .await
                .is_ok()
            {
                let _ = reader.get_mut().write_all(body).await;
            }
        }
    });
    (format!("http://{address}/announce"), requests)
}

fn assert_started_announce(request_line: &str, expected_left: usize) {
    let target = request_line
        .split_whitespace()
        .nth(1)
        .expect("tracker request target");
    let url = url::Url::parse(&format!("http://tracker.invalid{target}")).unwrap();
    let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(query.get("event").map(String::as_str), Some("started"));
    assert_eq!(query.get("uploaded").map(String::as_str), Some("0"));
    assert_eq!(query.get("downloaded").map(String::as_str), Some("0"));
    assert_eq!(
        query
            .get("left")
            .and_then(|value| value.parse::<usize>().ok()),
        Some(expected_left),
    );
}

#[tokio::test]
async fn private_torrent_never_announces_to_session_public_trackers() {
    let (private_tracker, private_requests) = tracker().await;
    let (public_tracker, public_requests) = tracker().await;
    let dir = tempfile::tempdir().unwrap();
    let engine = TorrentEngine::start(
        &dir.path().join("state"),
        dir.path().join("downloads"),
        &|_| true,
        vec![public_tracker],
        false,
        0,
    )
    .await
    .unwrap();

    engine
        .add(
            &TorrentSource::File(torrent(&private_tracker, true)),
            &dir.path().join("downloads"),
            None,
            false,
        )
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(10), async {
        while private_requests.lock().unwrap().requests.is_empty() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("private torrent did not announce to its own tracker");
    let captured = private_requests.lock().unwrap();
    let announce = captured.requests[0].clone();
    assert_started_announce(&announce, b"private test payload".len());
    let target = announce.split_whitespace().nth(1).unwrap();
    let url = url::Url::parse(&format!("http://tracker.invalid{target}")).unwrap();
    let query: HashMap<_, _> = url.query_pairs().into_owned().collect();
    let version = env!("CARGO_PKG_VERSION").split('.').collect::<Vec<_>>();
    let fingerprint = format!("-Pr{}{}{}0-", version[0], version[1], version[2]);
    assert!(query["peer_id"].starts_with(&fingerprint));
    assert_eq!(
        captured.user_agents[0],
        format!("Prism/{}", env!("CARGO_PKG_VERSION"))
    );
    drop(captured);
    // If the session's public list were appended, it would be announced in the same start.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(public_requests.lock().unwrap().requests.is_empty());
}

#[tokio::test]
async fn public_torrent_still_uses_session_public_trackers() {
    let (torrent_tracker, torrent_requests) = tracker().await;
    let (public_tracker, public_requests) = tracker().await;
    let dir = tempfile::tempdir().unwrap();
    let engine = TorrentEngine::start(
        &dir.path().join("state"),
        dir.path().join("downloads"),
        &|_| true,
        vec![public_tracker],
        false,
        0,
    )
    .await
    .unwrap();

    engine
        .add(
            &TorrentSource::File(torrent(&torrent_tracker, false)),
            &dir.path().join("downloads"),
            None,
            false,
        )
        .await
        .unwrap();

    tokio::time::timeout(Duration::from_secs(10), async {
        while torrent_requests.lock().unwrap().requests.is_empty()
            || public_requests.lock().unwrap().requests.is_empty()
        {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("public torrent did not announce to both its own and configured trackers");
}
