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
                            let past = AtomicOptionInstant::none();
                            let triggered = AtomicBool::new(false);
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
                                        &triggered,
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
    triggered: &AtomicBool,
    datarate: &AccelDataRate,
) {
    if !triggered.load(Ordering::Relaxed) {
        triggered.store(true, Ordering::Relaxed);
        log::debug!("Accelerometer callback triggered for device at index {index}");
    }
    let now = Instant::now();
    let gap = past
        .swap(Some(now), Ordering::Relaxed)
        .map(|past| now.duration_since(past).as_micros() as u32)
        .unwrap_or(0);
    datarate.last.compare_exchange(None, Some(now), Ordering::Relaxed, Ordering::Relaxed).ok(); // Update last timestamp if it was None
    datarate.count.fetch_add(1, Ordering::Relaxed); // Increment count
    if let Some(last) = datarate.last.load(Ordering::Relaxed) {
        let elapsed = now.duration_since(last).as_secs_f32();
        if elapsed > 1.0 {
            datarate.last.store(Some(now), Ordering::Relaxed); // Update last timestamp
            let count = datarate.count.swap(0, Ordering::Relaxed); // Reset count
            log::debug!("[ACCEL] Device {index} data rate: {:.3} Hz", count as f32 / elapsed);
        }
    }

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
