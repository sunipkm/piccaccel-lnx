//! ADXL355 embedded-hal SPI driver crate
//!
//! A platform agnostic driver to interface with the ADXL355 Accelerometer.
//! This driver uses SPI via [embedded-hal] and implements the [`Accelerometer` trait][trait]
//! from the `accelerometer` crate.
//!
//! [embedded-hal]: https://docs.rs/embedded-hal
//! [trait]: https://docs.rs/accelerometer/latest/accelerometer/trait.Accelerometer.html
//!
//!
//! # Usage
//!
//! Use embedded-hal implementation to get SPI and a GPIO OutputPin for the chip select,
//! then create the accelerometer handle
//!
//! ```
//!
//! use adxl355::{Adxl355, Config as ADXLConfig, ODR_LPF, Range, Accelerometer};
//!
//! // to create sensor with default configuration:
//! let mut accelerometer = Adxl355::default(spi, cs)?;
//!
//! // start measurements
//! accelerometer.start();
//!
//! // to get 3d accerlation data:
//! let accel = accelerometer.accel_norm()?;
//! println!("{:?}", accel);
//!
//! // One can also use conf module to supply configuration:
//!
//! let mut accelerometer =
//!     Adxl355::new(spi, cs,
//!                     ADXLConfig::new()
//!                     .odr(ODR_LPF::ODR_31_25_Hz)
//!                     .range(Range::_2G))?;
//! ```
//!
//! # References
//!
//! - [Register Map][1]
//!
//! [1]: https://www.analog.com/media/en/technical-documentation/data-sheets/adxl354_355.pdf
//!
//! - [`embedded-hal`][2]
//!
//! [2]: https://github.com/rust-embedded/embedded-hal
//!
//!

#![no_std]

mod conf;
mod register;

use core::fmt::Debug;

use embedded_hal as hal;

use hal::spi::SpiBus;

pub use accelerometer::{
    error,
    vector::{F32x3, I32x3},
    Accelerometer, Error, RawAccelerometer,
};

pub use conf::*;
use register::Register;

const SPI_READ: u8 = 0x01;
const SPI_WRITE: u8 = 0x00;

const EXPECTED_DEVICE_ID: u8 = 0xED;

const ACCEL_MAX_I20: u32 = 524_287; // = 2^(20-1)-1

/// ADXL355 driver
pub struct Adxl355<SPI> {
    spi: SPI,

    // configuration
    odr: ODR_LPF,
    hpf: HPF_CORNER,
    range: Range,
}

impl<SPI, E> Adxl355<SPI>
where
    SPI: SpiBus<u8, Error = E>,
{
    /// Creates a new `adxl355` driver from a SPI peripheral with
    /// default configuration.
    pub fn default(spi: SPI) -> Result<Self, E> {
        Adxl355::new(spi, &Config::default())
    }

    /// Takes a config object to initialize the adxl355 driver
    pub fn new(spi: SPI, config: &Config) -> Result<Self, E> {
        let mut adxl355 = Adxl355 {
            spi,
            odr: config.odr.unwrap_or_default(),
            hpf: config.hpf.unwrap_or_default(),
            range: config.range.unwrap_or_default(),
        };

        let id = adxl355.get_device_id()?;

        if id != EXPECTED_DEVICE_ID {
            // error
        }

        adxl355.write_reg(
            Register::FILTER.addr(),
            (adxl355.hpf.val() << 4) | adxl355.odr.val(),
        )?;
        adxl355.write_reg(Register::RANGE.addr(), adxl355.range.val())?;

        Ok(adxl355)
    }

    /// Puts the device in `Measurement mode`. The defaut after power up is `Standby mode`.
    pub fn start(&mut self) -> Result<(), E> {
        self.write_reg(Register::POWER_CTL.addr(), 0)
    }

    /// Returns the raw contents of the temperature registers
    pub fn read_temp_raw(&mut self) -> u16 {
        let mut bytes = [(Register::TEMP2.addr() << 1) | SPI_READ, 0, 0];
        self.read(&mut bytes);

        let temp_h = ((bytes[1] & 0x0F) as u16) << 8;
        let temp_l = (bytes[2] as u16) & 0x00FF;

        temp_h | temp_l
    }

    /// Get the device ID
    pub fn get_device_id(&mut self) -> Result<u8, E> {
        let reg = Register::DEVID.addr();
        let mut output = [1u8];
        self.read_reg(reg, &mut output)?;
        Ok(output[0])
    }

    fn write_reg(&mut self, reg: u8, value: u8) -> Result<(), E> {
        let bytes = [(reg << 1) | SPI_WRITE, value];
        self.spi.write(&bytes)?;
        Ok(())
    }

    fn read_reg(&mut self, reg: u8, buffer: &mut [u8]) -> Result<(), E> {
        let mut bytes = [(reg << 1) | SPI_READ, 0];
        self.spi.transfer_in_place(&mut bytes)?;
        buffer[0] = bytes[1];
        Ok(())
    }

    fn read(&mut self, bytes: &mut [u8]) {
        self.spi.transfer_in_place(bytes).ok();
    }
}

impl<SPI, E> RawAccelerometer<I32x3> for Adxl355<SPI>
where
    SPI: SpiBus<u8, Error = E>,
    E: Debug,
{
    type Error = E;

    /// Gets acceleration vector reading from the accelerometer
    /// Returns a 3D vector with x,y,z, fields in a Result
    fn accel_raw(&mut self) -> Result<I32x3, Error<E>> {
        let mut bytes = [0u8; 9 + 1];
        bytes[0] = (Register::XDATA3.addr() << 1) | SPI_READ;
        self.read(&mut bytes);

        // combine 3 bytes into one i32 value
        // right-shift with sign-extend to 20-bit
        #[allow(clippy::double_parens)]
        {
            let x = (((bytes[1] as i32) << 24)
                | ((bytes[2] as i32) << 16)
                | ((bytes[3] & 0xF0) as i32) << 8)
                >> 12;
            let y = (((bytes[4] as i32) << 24)
                | ((bytes[5] as i32) << 16)
                | ((bytes[6] & 0xF0) as i32) << 8)
                >> 12;
            let z = (((bytes[7] as i32) << 24)
                | ((bytes[8] as i32) << 16)
                | ((bytes[9] & 0xF0) as i32) << 8)
                >> 12;

            Ok(I32x3::new(x, y, z))
        }
    }
}

impl<SPI, E> Accelerometer for Adxl355<SPI>
where
    SPI: SpiBus<u8, Error = E>,
    E: Debug,
{
    type Error = E;

    fn sample_rate(&mut self) -> Result<f32, Error<Self::Error>> {
        Ok(self.odr.into())
    }

    fn accel_norm(&mut self) -> Result<F32x3, Error<Self::Error>> {
        let raw_data: I32x3 = self.accel_raw()?;
        let range: f32 = self.range.into(); // range in [g], so 2, 4 or 8

        let x = (raw_data.x as f32 / ACCEL_MAX_I20 as f32) * range;
        let y = (raw_data.y as f32 / ACCEL_MAX_I20 as f32) * range;
        let z = (raw_data.z as f32 / ACCEL_MAX_I20 as f32) * range;

        Ok(F32x3::new(x, y, z))
    }
}
