//! USB display driver for Ableton Push 2.
//!
//! The Push 2 display uses a USB bulk transfer protocol:
//! - Vendor ID: 0x2982
//! - Product ID: 0x1967
//! - Frame format: BGR565, 960x160 pixels
//! - XOR mask: 0xFFE7F3E7 applied to frame data
//! - Transfer: 640 buffers of 512 bytes each

use rusb::{Context, DeviceHandle, UsbContext};
use thiserror::Error;

use super::FrameBuffer;

/// Push 2 USB identifiers
const PUSH2_VENDOR_ID: u16 = 0x2982;
const PUSH2_PRODUCT_ID: u16 = 0x1967;

/// USB endpoint for display data
const DISPLAY_ENDPOINT: u8 = 0x01;

/// Frame header sent before each frame
const FRAME_HEADER: [u8; 16] = [
    0xFF, 0xCC, 0xAA, 0x88, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
];

/// XOR mask for frame data (applied as u32 words)
const XOR_MASK: [u8; 4] = [0xE7, 0xF3, 0xE7, 0xFF];

/// USB transfer timeout in milliseconds
const USB_TIMEOUT_MS: u64 = 1000;

/// Errors that can occur with the Push 2 display.
#[derive(Debug, Error)]
pub enum Push2DisplayError {
    #[error("Push 2 device not found")]
    DeviceNotFound,

    #[error("USB error: {0}")]
    UsbError(#[from] rusb::Error),

    #[error("Failed to claim interface")]
    InterfaceClaim,

    #[error("Frame transfer failed")]
    TransferFailed,
}

/// Push 2 USB display driver.
pub struct Push2Display {
    handle: DeviceHandle<Context>,
    interface_claimed: bool,
}

impl Push2Display {
    /// Create a new Push2Display by connecting to the device.
    pub fn new() -> Result<Self, Push2DisplayError> {
        let context = Context::new()?;

        // Find Push 2 device
        let device = context
            .devices()?
            .iter()
            .find(|d| {
                d.device_descriptor().map_or(false, |desc| {
                    desc.vendor_id() == PUSH2_VENDOR_ID && desc.product_id() == PUSH2_PRODUCT_ID
                })
            })
            .ok_or(Push2DisplayError::DeviceNotFound)?;

        // Open device
        let mut handle = device.open()?;

        // Claim interface
        let interface_claimed = handle.claim_interface(0).is_ok();
        if !interface_claimed {
            tracing::warn!("Could not claim USB interface - display may not work");
        }

        Ok(Self {
            handle,
            interface_claimed,
        })
    }

    /// Send a frame to the display.
    pub fn send_frame(&mut self, frame_buffer: &FrameBuffer) -> Result<(), Push2DisplayError> {
        if !self.interface_claimed {
            return Err(Push2DisplayError::InterfaceClaim);
        }

        let timeout = std::time::Duration::from_millis(USB_TIMEOUT_MS);

        // Send frame header
        self.handle
            .write_bulk(DISPLAY_ENDPOINT, &FRAME_HEADER, timeout)?;

        // Get encoded frame data
        let frame_data = frame_buffer.to_usb_frame();

        // Send frame data in 512-byte chunks
        for chunk in frame_data.chunks(512) {
            self.handle.write_bulk(DISPLAY_ENDPOINT, chunk, timeout)?;
        }

        Ok(())
    }

    /// Apply XOR mask to frame data (in-place).
    pub fn apply_xor_mask(data: &mut [u8]) {
        for (i, byte) in data.iter_mut().enumerate() {
            *byte ^= XOR_MASK[i % 4];
        }
    }
}

impl Drop for Push2Display {
    fn drop(&mut self) {
        if self.interface_claimed {
            let _ = self.handle.release_interface(0);
        }
    }
}
