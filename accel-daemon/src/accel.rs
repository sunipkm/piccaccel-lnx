use adxl355::{Adxl355, Config as ADXLConfig, F32x3, Range, ODR_LPF};
use rppal::gpio::Gpio;
use rppal::spi::{Bus, Mode, SlaveSelect, Spi};
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

const ACCEL_ODR: ODR_LPF = ODR_LPF::ODR_1000_Hz;

pub struct AccelDesc {
    bus: Bus,
    ss: SlaveSelect,
    drdy: u8,
}

pub fn accelerator_init(
    acceldescs: &[AccelDesc],
    running: Arc<AtomicBool>,
) -> Result<(), Box<dyn Error>> {
    let gpio = Gpio::new()?;
    let mut pins = acceldescs
        .iter()
        .enumerate()
        .filter_map(|(index, acceldesc)| {
            if let Ok(drdy) = gpio.get(acceldesc.drdy) {
                log::info!(
                    "DRDY pin {} found, initializing accel on bus {:?}",
                    acceldesc.drdy,
                    acceldesc.bus
                );
                let mut drdy = drdy.into_input();
                if let Ok(spi) = Spi::new(
                    acceldesc.bus,
                    acceldesc.ss,
                    1_000_000, // 1 MHz
                    Mode::Mode0,
                ) {
                    if let Ok(mut accel) = adxl355::Adxl355::new(
                        spi,
                        ADXLConfig::default()
                            .odr(ACCEL_ODR)
                            .hpf(adxl355::HPF_CORNER::_0_238_ODR)
                            .range(Range::_2G),
                    ) {
                        if let Err(e) = accel.start() {
                            log::error!("Failed to start accel: {}", e);
                            None
                        } else if let Err(e) = drdy.set_async_interrupt(rppal::gpio::Trigger::FallingEdge, None, |_| {}) {
                            log::error!("Failed to set async interrupt for pin {}: {}", acceldesc.drdy, e);
                            None
                        } else {
                            log::info!("Accelerometer on bus {:?} initialized successfully.", acceldesc.bus);
                            Some(drdy)
                        }
                    } else {
                        log::error!(
                            "Failed to create ADXL355 instance on bus {:?}",
                            acceldesc.bus
                        );
                        None
                    }
                } else {
                    log::error!("Failed to initialize SPI on bus {:?}", acceldesc.bus);
                    None
                }
            } else {
                log::info!(
                    "DRDY pin {} not found, skipping accel on bus {:?}",
                    acceldesc.drdy,
                    acceldesc.bus
                );
                None
            }
        })
        .collect::<Vec<_>>();
    if pins.is_empty() {
        log::warn!("No accelerometer DRDY pins found, exiting thread.");
        return Ok(());
    }
    log::info!("Found {} accelerometer DRDY pins.", pins.len());
    while running.load(Ordering::Relaxed) {
        thread::sleep(Duration::from_secs(1));
    }
    log::info!("Stopping accelerometer thread.");
    for pin in pins.iter_mut() {
        if let Err(e) = pin.clear_async_interrupt() {
            log::error!("Failed to clear async interrupt for pin {:?}: {}", pin, e);
        }
    }
    Ok(())
}

fn accelerator_callback(index: usize, device: Adxl355<Spi>, sink: tokio::sync::broadcast::Sender<(usize, F32x3)>) {
    
}