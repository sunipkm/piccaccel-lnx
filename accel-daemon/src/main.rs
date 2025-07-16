mod accel;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[allow(unused_imports)]
use accel::{AccelDesc, accelerator_init, accelerator_task};
use accel_data::tcp_server;

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
    #[allow(clippy::useless_vec)]
    let acceldescs = vec![AccelDesc {
        bus: Bus::Spi1,
        ss: SlaveSelect::Ss2,
        drdy: 19, // GPIO pin for data ready
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
    // let mut pins = match accelerator_init(&acceldescs, sink.clone()) {
    //     Ok(pins) => {
    //         log::info!("Accelerometer initialized with {} pins", pins.len());
    //         pins
    //     }
    //     Err(e) => {
    //         log::error!("Failed to initialize accelerometer: {e}");
    //         return;
    //     }
    // };
    let hdls = acceldescs
        .iter()
        .enumerate()
        .map(|(index, acceldesc)| {
            let sink = sink.clone();
            let acceldesc = acceldesc.clone();
            tokio::spawn({
                let running = running.clone();
                async move {
                    accelerator_task(index as u32, acceldesc, sink, running).await;
                }
            })
        })
        .collect::<Vec<_>>();
    // Start the TCP server
    let srv_task = tokio::spawn(tcp_server(args.port, running.clone(), sink));
    log::info!("TCP server started on port {}", args.port);
    // Wait
    while running.load(Ordering::Relaxed) {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
    }
    log::info!("Stopping TCP server...");
    srv_task.abort();
    log::info!("Server stopped, exiting...");
    // Clean up GPIO pins
    // for mut pin in pins.drain(..) {
    //     if let Err(e) = pin.clear_async_interrupt() {
    //         log::error!("Failed to clear async interrupt for {pin:?}: {e}");
    //     } else {
    //         log::info!("Cleared async interrupt for {pin:?}");
    //     }
    // }
    for hdl in hdls {
        if let Err(e) = hdl.await {
            log::error!("Accelerometer initialization task failed: {e}");
        }
    }
    if let Err(e) = srv_task.await {
        log::error!("TCP server task failed: {e}");
    } else {
        log::info!("TCP server task completed successfully");
    }
}
