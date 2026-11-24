//! Private torrents must not inherit session-wide public tracker URLs.
//! This protects the central privacy boundary of #109 without contacting the public internet.

use prism_torrent::{TorrentEngine, TorrentSource};
use sha1::{Digest, Sha1};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
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

async fn tracker() -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let count = requests.clone();
    tokio::spawn(async move {
        while let Ok((mut stream, _)) = listener.accept().await {
            count.fetch_add(1, Ordering::SeqCst);
            let mut request = vec![0; 4096];
            let _ = stream.read(&mut request).await;
            let body = b"d8:intervali30e5:peers0:e";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            if stream.write_all(response.as_bytes()).await.is_ok() {
                let _ = stream.write_all(body).await;
            }
        }
    });
    (format!("http://{address}/announce"), requests)
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
        while private_requests.load(Ordering::SeqCst) == 0 {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("private torrent did not announce to its own tracker");
    // If the session's public list were appended, it would be announced in the same start.
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(public_requests.load(Ordering::SeqCst), 0);
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
        while torrent_requests.load(Ordering::SeqCst) == 0
            || public_requests.load(Ordering::SeqCst) == 0
        {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("public torrent did not announce to both its own and configured trackers");
}
