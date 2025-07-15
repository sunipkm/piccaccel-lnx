mod accel;
mod net;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use accel::{AccelDesc, accelerator_init};
use net::tcp_server;

use clap::Parser;
use rppal::spi::{Bus, SlaveSelect};
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
    // Accelerometer descriptors
    let acceldescs = vec![AccelDesc {
        bus: Bus::Spi0,
        ss: SlaveSelect::Ss0,
        drdy: 17,
    }];
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
    // Initialize the accelerometer
    let mut pins = match accelerator_init(&acceldescs, sink.clone()) {
        Ok(pins) => {
            log::info!("Accelerometer initialized with {} pins", pins.len());
            pins
        }
        Err(e) => {
            log::error!("Failed to initialize accelerometer: {e}");
            return;
        }
    };
    // Start the TCP server
    tcp_server(args.port, running, sink).await;
    log::info!("Server stopped, exiting...");
    // Clean up GPIO pins
    for mut pin in pins.drain(..) {
        if let Err(e) = pin.clear_async_interrupt() {
            log::error!("Failed to clear async interrupt for {pin:?}: {e}");
        } else {
            log::info!("Cleared async interrupt for {pin:?}");
        }
    }
}
