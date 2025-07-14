use adxl355::{Accelerometer, Adxl355, Config as ADXLConfig, F32x3, ODR_LPF, Range};
use atomic_time::AtomicOptionInstant;
use rppal::gpio::Gpio;
use rppal::spi::{Bus, Mode, SlaveSelect, Spi};
use std::error::Error;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::broadcast::Sender;

const ACCEL_ODR: ODR_LPF = ODR_LPF::ODR_1000_Hz;

pub struct AccelDesc {
    bus: Bus,
    ss: SlaveSelect,
    drdy: u8,
}

pub fn accelerator_init(
    acceldescs: &[AccelDesc],
    running: Arc<AtomicBool>,
    sink: Sender<(u32, u32, F32x3)>,
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
                            log::error!("Failed to start accel: {e}");
                            None
                        } else if let Err(e) = {
                            let sink = sink.clone();
                            let past = AtomicOptionInstant::none();
                            drdy.set_async_interrupt(
                                rppal::gpio::Trigger::FallingEdge,
                                None,
                                move |_| {
                                    accelerator_callback(index as u32, &mut accel, &past, &sink)
                                },
                            )
                        } {
                            log::error!(
                                "Failed to set async interrupt for pin {}: {}",
                                acceldesc.drdy,
                                e
                            );
                            None
                        } else {
                            log::info!(
                                "Accelerometer on bus {:?} initialized successfully.",
                                acceldesc.bus
                            );
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
            log::error!("Failed to clear async interrupt for pin {pin:?}: {e}");
        }
    }
    Ok(())
}

fn accelerator_callback(
    index: u32,
    device: &mut Adxl355<Spi>,
    past: &AtomicOptionInstant,
    sink: &Sender<(u32, u32, F32x3)>,
) {
    let now = Instant::now();
    let gap = past
        .swap(Some(now), Ordering::Relaxed)
        .map(|past| now.duration_since(past).as_micros() as u32)
        .unwrap_or(0);

    if let Ok(data) = device.accel_norm() {
        if let Err(e) = sink.send((index, gap, data)) {
            log::error!("Failed to send accelerometer data: {e}");
        }
    } else {
        log::error!("Failed to read accelerometer data from device at index {index}",);
    }
}
