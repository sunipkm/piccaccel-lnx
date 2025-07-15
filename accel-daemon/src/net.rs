use crate::accel::AccelData;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast::Sender;

pub async fn tcp_server(port: u16, running: Arc<AtomicBool>, sink: Sender<AccelData>) {
    log::info!("[NET] Starting TCP server on port {port}");
    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{port}"))
        .await
        .expect("[NET] Failed to bind TCP listener");
    log::info!("[NET] TCP server listening on port {}", port);
    while running.load(Ordering::Relaxed) {
        match listener.accept().await {
            Ok((socket, addr)) => {
                log::info!("[NET] Accepted connection from {}", addr);
                let running = running.clone();
                let sink = sink.clone();
                tokio::spawn(async move {
                    handle_client(socket, addr, running, sink).await;
                });
            }
            Err(e) => {
                log::error!("[NET] Failed to accept connection on server: {e}");
            }
        }
    }
    log::info!("[NET] TCP server stopped");
}

async fn handle_client(
    socket: tokio::net::TcpStream,
    addr: std::net::SocketAddr,
    running: Arc<AtomicBool>,
    sink: Sender<AccelData>,
) {
    log::info!("[NET] {addr}> Handling client.");
    let mut source = sink.subscribe();
    let (_, mut writer) = socket.into_split();
    while running.load(Ordering::Relaxed) {
        match source.recv().await {
            Ok(data) => {
                if writer
                    .write_all(serde_json::to_string(&data).unwrap().as_bytes())
                    .await
                    .is_err()
                {
                    log::error!("[NET] {addr}> Failed to write data to client");
                    break;
                }
            }
            Err(e) => {
                log::error!("[NET] {addr}> Error receiving data: {e}");
                break;
            }
        }
    }
}
