use cpal::traits::{DeviceTrait, HostTrait};
use serde::{Deserialize, Serialize};

/// Information about an audio device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
}

/// Enumerate all available audio output devices
pub fn enumerate_audio_devices() -> Result<Vec<AudioDeviceInfo>, String> {
    let host = cpal::default_host();

    // Get the default output device name
    let default_device_name = host.default_output_device().and_then(|d| d.name().ok());

    // Enumerate all output devices
    let devices = host
        .output_devices()
        .map_err(|e| format!("Failed to enumerate audio devices: {e}"))?;

    let mut device_list = Vec::new();

    for device in devices {
        if let Ok(name) = device.name() {
            let is_default = default_device_name.as_ref() == Some(&name);
            device_list.push(AudioDeviceInfo { name, is_default });
        }
    }

    // If no devices found, add a fallback
    if device_list.is_empty() {
        device_list.push(AudioDeviceInfo {
            name: "Default".to_string(),
            is_default: true,
        });
    }

    Ok(device_list)
}

/// Get the default audio device name
pub fn get_default_audio_device() -> String {
    let host = cpal::default_host();
    host.default_output_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_else(|| "Default".to_string())
}
