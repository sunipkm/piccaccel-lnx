#![allow(dead_code)]
use accel_data::AccelData;
use adxl355::{Accelerometer, Adxl355, Config as ADXLConfig, ODR_LPF, Range};
use atomic_time::AtomicOptionInstant;
use rppal::gpio::{Gpio, InputPin};
use rppal::spi::{Bus, Mode, SlaveSelect, Spi};
use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::broadcast::Sender;

const ACCEL_ODR: ODR_LPF = ODR_LPF::ODR_1000_Hz;

/// Convert ODR_LPF to microseconds
const fn get_odr(odr: ODR_LPF) -> u32 {
    match odr {
        ODR_LPF::ODR_4000_Hz => 250,
        ODR_LPF::ODR_2000_Hz => 500,
        ODR_LPF::ODR_1000_Hz => 1000,
        ODR_LPF::ODR_500_Hz => 2000,
        ODR_LPF::ODR_250_Hz => 4000,
        ODR_LPF::ODR_125_Hz => 8000,
        ODR_LPF::ODR_62_5_Hz => 16000,
        ODR_LPF::ODR_31_25_Hz => 32000,
        ODR_LPF::ODR_15_625_Hz => 64000,
        ODR_LPF::ODR_7_813_Hz => 128000,
        ODR_LPF::ODR_3_906_Hz => 256000,
    }
}

#[derive(Debug, Clone)]
pub struct AccelDesc {
    pub bus: Bus,
    pub ss: SlaveSelect,
    pub drdy: u8,
}

struct AccelDataRate {
    last: AtomicOptionInstant,
    count: AtomicUsize,
}

pub fn accelerator_init(
    acceldescs: &[AccelDesc],
    sink: Sender<AccelData>,
) -> Result<Vec<InputPin>, Box<dyn Error>> {
    log::debug!(
        "AtomicOptionInstant is lock free? {}",
        AtomicOptionInstant::is_lock_free()
    );
    let now = Instant::now(); // synchronization point
    let gpio = Gpio::new()?;
    let pins = acceldescs
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
                            thread::sleep(Duration::from_millis(100));
                            if let Ok(value) = accel.accel_norm() {
                                log::info!("Accelerometer {acceldesc:?} data: {value:?}");
                            }
                            let sink = sink.clone();
                            let past = AtomicOptionInstant::new(Some(now));
                            let datarate = AccelDataRate {
                                last: AtomicOptionInstant::none(),
                                count: AtomicUsize::new(0),
                            };
                            drdy.set_async_interrupt(
                                rppal::gpio::Trigger::FallingEdge,
                                None,
                                move |_| {
                                    accelerator_callback(
                                        index as u32,
                                        &mut accel,
                                        &past,
                                        &sink,
                                        &datarate,
                                    )
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
    }
    Ok(pins)
}

fn accelerator_callback(
    index: u32,
    device: &mut Adxl355<Spi>,
    past: &AtomicOptionInstant,
    sink: &Sender<AccelData>,
    datarate: &AccelDataRate,
) {
    let now = Instant::now();
    datarate.count.fetch_add(1, Ordering::Relaxed); // Increment count
    // At the first call, we get time from the synchronization point
    let gap = past
        .swap(None, Ordering::Relaxed)
        .map(|past| {
            log::debug!("Accelerometer callback triggered for device at index {index}");
            now.duration_since(past).as_micros() as u32
        })
        .unwrap_or(get_odr(ACCEL_ODR)); // otherwise, we use the output data rate
    // Update the tick count
    datarate
        .last
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |past| {
            match past {
                None => Some(Some(now)), // first call, set to current time
                Some(past) => {
                    let dur = now.duration_since(past).as_secs_f32();
                    if dur < 1.0 {
                        None // Keep the old timestamp if less than 1 second has passed
                    } else {
                        let count = datarate.count.swap(0, Ordering::Relaxed); // Reset count
                        log::debug!(
                            "[ACCEL] Device {index} data rate: {:.3} Hz",
                            count as f32 / dur
                        );
                        Some(Some(now)) // Update to the current time
                    }
                }
            }
        })
        .ok();

    if let Ok(data) = device.accel_norm() {
        if sink.receiver_count() > 0
            && sink
                .send(AccelData {
                    idx: index,
                    gap,
                    x: data.x,
                    y: data.y,
                    z: data.z,
                })
                .is_err()
        {
            log::error!("Failed to send accelerometer data for device at index {index}");
        }
    } else {
        log::error!("Failed to read accelerometer data from device at index {index}",);
    }
}

pub async fn accelerator_task(
    index: u32,
    acceldesc: AccelDesc,
    sink: Sender<AccelData>,
    running: Arc<AtomicBool>,
) {
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
            } else {
                thread::sleep(Duration::from_millis(100));
                if let Ok(value) = accel.accel_norm() {
                    log::info!("Accelerometer {acceldesc:?} data: {value:?}");
                }
                let mut now = Instant::now();
                while running.load(Ordering::Relaxed) {
                    if let Ok(data) = accel.accel_norm() {
                        let tnow = Instant::now();
                        let dur = tnow.duration_since(now).as_micros() as u32;
                        now = tnow;
                        if sink.receiver_count() > 0
                            && sink
                                .send(AccelData {
                                    idx: index,
                                    gap: dur,
                                    x: data.x,
                                    y: data.y,
                                    z: data.z,
                                })
                                .is_err()
                        {
                            log::error!(
                                "Failed to send accelerometer data for device at index {index}"
                            );
                        }
                    } else {
                        log::error!(
                            "Failed to read accelerometer data from device at index {index}"
                        );
                    }
                    thread::sleep(Duration::from_micros(900)); // Adjust as needed
                }
            }
        } else {
            log::error!(
                "Failed to create ADXL355 instance on bus {:?}",
                acceldesc.bus
            );
        }
    } else {
        log::error!(
            "Failed to create ADXL355 instance on bus {:?}",
            acceldesc.bus
        );
    }
}
