use micromath::vector::F32x3;
mod net;

pub use net::tcp_server;
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
#[repr(C, packed)]
/// Accelerometer data structure
pub struct AccelData {
    /// Sensor Index
    pub idx: u32,
    /// Time in microseconds since the last data point
    /// (0 if this is the first data point)
    pub gap: u32,
    /// X-axis acceleration
    pub x: f32,
    /// Y-axis acceleration
    pub y: f32,
    /// Z-axis acceleration
    pub z: f32,
}

impl From<(u32, u32, F32x3)> for AccelData {
    fn from(val: (u32, u32, F32x3)) -> Self {
        AccelData {
            idx: val.0,
            gap: val.1,
            x: val.2.x,
            y: val.2.y,
            z: val.2.z,
        }
    }
}

impl AccelData {
    pub fn as_bytes(self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(std::mem::size_of::<AccelData>());
        buf.extend_from_slice(&self.idx.to_le_bytes());
        buf.extend_from_slice(&self.gap.to_le_bytes());
        buf.extend_from_slice(&self.x.to_le_bytes());
        buf.extend_from_slice(&self.y.to_le_bytes());
        buf.extend_from_slice(&self.z.to_le_bytes());
        buf
    }
}
