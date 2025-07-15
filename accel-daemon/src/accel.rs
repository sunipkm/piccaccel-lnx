use accel_data::AccelData;
use adxl355::{Accelerometer, Adxl355, Config as ADXLConfig, ODR_LPF, Range};
use atomic_time::AtomicOptionInstant;
use rppal::gpio::{Gpio, InputPin};
use rppal::spi::{Bus, Mode, SlaveSelect, Spi};
use std::error::Error;
use std::sync::atomic::Ordering;
use std::time::Instant;
use tokio::sync::broadcast::Sender;

const ACCEL_ODR: ODR_LPF = ODR_LPF::ODR_1000_Hz;

pub struct AccelDesc {
    pub bus: Bus,
    pub ss: SlaveSelect,
    pub drdy: u8,
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
    }
    Ok(pins)
}

fn accelerator_callback(
    index: u32,
    device: &mut Adxl355<Spi>,
    past: &AtomicOptionInstant,
    sink: &Sender<AccelData>,
) {
    let now = Instant::now();
    let gap = past
        .swap(Some(now), Ordering::Relaxed)
        .map(|past| now.duration_since(past).as_micros() as u32)
        .unwrap_or(0);

    if let Ok(data) = device.accel_norm() {
        if sink.receiver_count() > 0 && sink.send(AccelData {
            idx: index,
            gap,
            x: data.x,
            y: data.y,
            z: data.z,
        }).is_err() {
            log::error!("Failed to send accelerometer data for device at index {index}");
        }
    } else {
        log::error!("Failed to read accelerometer data from device at index {index}",);
    }
}
