use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

use accel_data::{tcp_server, AccelData};
use clap::Parser;
/// Program to forward serial port over TCP
#[derive(Parser, Debug)]
#[command(version, about, long_about)]
struct Args {
    #[arg(
        short,
        long,
        default_value = "14389",
        value_parser = clap::value_parser!(u16).range(1..=65535)
    )]
    /// Network port to listen on to send commands
    port: u16,
}

#[tokio::main]
async fn main() {
    // Initialize the logger
    env_logger::init();
    // Parse command line arguments
    let args = Args::parse();
    log::info!("Arguments: {args:#?}");
    // Create a running flag
    let running = Arc::new(AtomicBool::new(true));
    // Handle Ctrl+C to stop the server gracefully
    let _ctrlchdl = tokio::spawn({
        let running = running.clone();
        async move {
            tokio::signal::ctrl_c().await.unwrap();
            running.store(false, Ordering::SeqCst);
        }
    });
    // Create a broadcast channel for sending accelerometer data
    let (sink, _) = tokio::sync::broadcast::channel(100);
    // Initialize dummy data source
    let gen_task = tokio::spawn(generate_dummy_data(
        0, // Dummy index
        running.clone(),
        sink.clone(),
    ));
    // Start the TCP server
    tcp_server(args.port, running, sink).await;
    log::info!("Server stopped, exiting...");
    // Wait for the dummy data generation task to finish
    if let Err(e) = gen_task.await {
        log::error!("Dummy data generation task failed: {e}");
    } else {
        log::info!("Dummy data generation task completed successfully");
    }
}

async fn generate_dummy_data(
    idx: u32,
    running: Arc<AtomicBool>,
    sink: tokio::sync::broadcast::Sender<AccelData>,
) {
    let root = Instant::now();
    let mut start = None;
    while running.load(Ordering::Relaxed) {
        let (gap, now) = match start.take() {
            None => {
                let now = Instant::now();
                start = Some(now);
                (0, now.duration_since(root).as_secs_f32())
            }
            Some(past) => {
                let now = Instant::now();
                let elapsed = now.duration_since(past).as_micros() as u32;
                start = Some(now);
                (elapsed, now.duration_since(root).as_secs_f32())
            }
        };
        let data = AccelData {
            idx,
            gap,
            x: f32::sin(now / 5.0),
            y: f32::cos(now / 5.0 + 1.0),
            z: f32::tan(now / 5.0 + 2.0).clamp(-2.0, 2.0),
        };
        if sink.send(data).is_err() {
            log::error!("Failed to send dummy data");
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
    }
}
