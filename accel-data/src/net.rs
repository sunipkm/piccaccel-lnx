use crate::AccelData;
use futures_util::{SinkExt, stream::StreamExt};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use tokio::{io::AsyncWriteExt, sync::broadcast::Sender};
use tokio_tungstenite::tungstenite::protocol::Message;

pub async fn udp_server(port: u16, running: Arc<AtomicBool>, sink: Sender<AccelData>) {
    log::info!("[NET] Starting UDP server on port {port}");
    let listener = tokio::net::UdpSocket::bind(format!("0.0.0.0:{port}"))
        .await
        .expect("[NET] Failed to bind UDP listener");
    log::info!("[NET] UDP server listening on port {port}");
    let mut source = sink.subscribe();
    while running.load(Ordering::Relaxed) {
        let mut buf = Vec::with_capacity(1024);
        match source.recv().await {
            Ok(data) => {
                let data = data.as_bytes();
                if buf.len() + data.len() < buf.capacity() {
                    buf.extend_from_slice(&data);
                } else {
                    if listener.send(&buf).await.is_err() {
                        log::error!("[NET] Failed to send data over UDP");
                        break;
                    }
                    buf.clear();
                    buf.extend_from_slice(&data);
                }
            }
            Err(e) => {
                log::error!("[NET] Failed to receive data: {e}");
            }
        }
    }
    log::info!("[NET] UDP server stopped");
}

pub async fn tcp_server(port: u16, running: Arc<AtomicBool>, sink: Sender<AccelData>) {
    log::info!("[NET] Starting TCP server on port {port}");
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .expect("[NET] Failed to bind TCP listener");
    log::info!("[NET] TCP server listening on port {port}");
    while running.load(Ordering::Relaxed) {
        match listener.accept().await {
            Ok((socket, addr)) => {
                log::info!("[NET] Accepted connection from {addr}");
                let running = running.clone();
                let sink = sink.clone();
                tokio::spawn(async move {
                    handle_client_tcp(socket, addr, running, sink).await;
                });
            }
            Err(e) => {
                log::error!("[NET] Failed to accept connection on server: {e}");
            }
        }
    }
    log::info!("[NET] TCP server stopped");
}

#[allow(dead_code)]
async fn handle_client_tcp(
    socket: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    running: Arc<AtomicBool>,
    sink: Sender<AccelData>,
) {
    log::info!("[NET] {addr}> Handling client.");
    let (_, mut writer) = socket.into_split();
    let mut source = sink.subscribe();
    let mut buf = Vec::with_capacity(1024);
    let mut counter = 0;
    let mut now = std::time::Instant::now();

    while running.load(Ordering::Relaxed) {
        match source.recv().await {
            Ok(data) => {
                let data = data.as_bytes();
                if buf.len() + data.len() < buf.capacity() {
                    buf.extend_from_slice(&data);
                } else {
                    if writer.write_all(&buf).await.is_err() {
                        log::error!("[NET] {addr}> Error sending data");
                        break;
                    }
                    let nnow = std::time::Instant::now();
                    let dur = nnow.duration_since(now).as_secs_f32();
                    if dur > 1.0 {
                        log::info!("[NET] {addr}> Packet rate: {} packets/s", counter as f32 / dur);
                        now = nnow;
                        counter = 0;
                    }
                    buf.clear();
                    buf.extend_from_slice(&data);
                }
                counter += 1;
            }
            Err(e) => {
                log::error!("[NET] {addr}> Error receiving data: {e}");
            }
        }
    }
}

#[allow(dead_code)]
async fn handle_client_wsock(
    socket: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    running: Arc<AtomicBool>,
    sink: Sender<AccelData>,
) {
    log::info!("[NET] {addr}> Handling client.");
    let ws_stream = match tokio_tungstenite::accept_async(socket).await {
        Ok(ws_stream) => ws_stream,
        Err(e) => {
            log::error!("[NET] {addr}> Failed to upgrade connection: {e}");
            return;
        }
    };
    let (mut outgoing, mut incoming) = ws_stream.split();
    let mut source = sink.subscribe();
    let mut counter = 0;
    let mut now = std::time::Instant::now();
    let mut buf = Vec::with_capacity(128);
    while running.load(Ordering::Relaxed) {
        tokio::select! {
            msg = source.recv() => {
                match msg {
                    Ok(data) => {
                        if buf.len() + 1 < buf.capacity() {
                            buf.push(data);
                        } else {
                            let msg = serde_json::to_string(&buf).unwrap();
                            log::debug!("[NET] {addr}> Sending data: {msg}");
                            if let Err(e) = outgoing.send(Message::from(
                                msg.as_str(),
                            )).await {
                                log::error!("[NET] {addr}> Error sending data: {e}");
                                break;
                            }
                            buf.clear();
                            buf.push(data);
                        }
                        counter += 1;
                    }
                    Err(e) => {
                        log::error!("[NET] {addr}> Error receiving data: {e}");
                        break;
                    }
                }
            },
            msg = incoming.next() => {
                if let Some(Ok(msg)) = msg {
                    log::info!("[NET] {addr}> Received message: {msg}");
                    if msg.is_close() {
                        log::info!("[NET] {addr}> Client disconnected.");
                        break;
                    }
                } else {
                    log::info!("[NET] {addr}> Client disconnected or error occurred.");
                    break;
                }
            }
        }
        if now.elapsed() > std::time::Duration::from_millis(1000) {
            log::info!("[NET] {addr}> Sent {counter} packets.");
            counter = 0;
            now = std::time::Instant::now();
        }
    }
}
