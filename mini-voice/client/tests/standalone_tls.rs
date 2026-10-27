//! Run with `cargo test -p mini-client --test standalone_tls` (not workspace-only).
//! A workspace build can accidentally obtain its TLS provider from mini-server.
use mini_client::{audio::Mixer, network};
use std::sync::{mpsc::sync_channel, Arc, Mutex};
use tokio::{
    io::AsyncReadExt,
    net::TcpListener,
    time::{timeout, Duration},
};

#[tokio::test]
async fn standalone_client_sends_tls_client_hello_without_panicking() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("wss://localhost:{}", listener.local_addr().unwrap().port());
    let (tx, _rx) = sync_channel(128);
    let bridge = network::start(
        &tokio::runtime::Handle::current(),
        endpoint,
        None,
        tx,
        Arc::new(|| {}),
        Arc::new(Mutex::new(Mixer::new())),
        None,
    );
    let (mut socket, _) = timeout(Duration::from_secs(5), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut header = [0; 5];
    let result = timeout(Duration::from_secs(5), socket.read_exact(&mut header)).await;
    bridge.task.abort();
    let task = bridge.task.await;
    assert!(
        !task.as_ref().is_err_and(|e| e.is_panic()),
        "TLS worker panicked: {task:?}"
    );
    result
        .expect("no TLS handshake received")
        .expect("connection closed before TLS handshake");
    assert_eq!(header[0], 22, "expected TLS handshake record");
    assert_eq!(header[1], 3, "expected TLS record version");
}
