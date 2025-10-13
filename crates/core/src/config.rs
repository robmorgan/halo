use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Settings;

/// Configuration manager for Halo settings
/// Provides a layered configuration system that separates schema, available options, and persisted
/// values Configuration is stored in config.json in the repository root by default
pub struct ConfigManager {
    config_path: PathBuf,
    settings: Settings,
}

/// Available configuration options with validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSchema {
    pub general: GeneralConfigSchema,
    pub audio: AudioConfigSchema,
    pub midi: MidiConfigSchema,
    pub output: OutputConfigSchema,
    pub fixture: FixtureConfigSchema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfigSchema {
    pub target_fps: ConfigOption<u32>,
    pub enable_autosave: ConfigOption<bool>,
    pub autosave_interval_secs: ConfigOption<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfigSchema {
    pub audio_device: ConfigOption<String>,
    pub audio_buffer_size: ConfigOption<u32>,
    pub audio_sample_rate: ConfigOption<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MidiConfigSchema {
    pub midi_enabled: ConfigOption<bool>,
    pub midi_device: ConfigOption<String>,
    pub midi_channel: ConfigOption<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputConfigSchema {
    pub dmx_enabled: ConfigOption<bool>,
    pub dmx_broadcast: ConfigOption<bool>,
    pub dmx_source_ip: ConfigOption<String>,
    pub dmx_dest_ip: ConfigOption<String>,
    pub dmx_port: ConfigOption<u16>,
    pub wled_enabled: ConfigOption<bool>,
    pub wled_ip: ConfigOption<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FixtureConfigSchema {
    pub enable_pan_tilt_limits: ConfigOption<bool>,
}

/// Configuration option with validation and available choices
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigOption<T> {
    pub default: T,
    pub valid_range: Option<(T, T)>,
    pub valid_choices: Option<Vec<T>>,
    pub description: String,
    pub requires_restart: bool,
}

/// Persisted configuration file format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigFile {
    pub version: String,
    pub settings: Settings,
    pub created_at: String,
    pub modified_at: String,
}

impl ConfigManager {
    /// Create a new configuration manager
    /// If no path is provided, defaults to 'config.json' in the current working directory
    pub fn new(config_path: Option<PathBuf>) -> Self {
        let config_path = config_path.unwrap_or_else(|| {
            // Default to config.json in the repository root
            PathBuf::from("config.json")
        });

        Self {
            config_path,
            settings: Settings::default(),
        }
    }

    /// Load settings from configuration file
    /// Returns default settings if file doesn't exist or is invalid
    pub fn load(&mut self) -> Result<Settings, ConfigError> {
        if !self.config_path.exists() {
            // Create default config file
            self.save()?;
            return Ok(self.settings.clone());
        }

        let content = fs::read_to_string(&self.config_path)
            .map_err(|e| ConfigError::ReadError(e.to_string()))?;

        let config_file: ConfigFile =
            serde_json::from_str(&content).map_err(|e| ConfigError::ParseError(e.to_string()))?;

        // Validate version compatibility
        if config_file.version != env!("CARGO_PKG_VERSION") {
            eprintln!(
                "Warning: Config file version {} doesn't match application version {}. Using defaults for new settings.",
                config_file.version,
                env!("CARGO_PKG_VERSION")
            );
        }

        self.settings = config_file.settings;
        Ok(self.settings.clone())
    }

    /// Save current settings to configuration file
    pub fn save(&self) -> Result<(), ConfigError> {
        // Ensure config directory exists (if config is in a subdirectory)
        if let Some(parent) = self.config_path.parent() {
            if parent != std::path::Path::new("") && parent != std::path::Path::new(".") {
                fs::create_dir_all(parent).map_err(|e| ConfigError::WriteError(e.to_string()))?;
            }
        }

        let config_file = ConfigFile {
            version: env!("CARGO_PKG_VERSION").to_string(),
            settings: self.settings.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            modified_at: chrono::Utc::now().to_rfc3339(),
        };

        let content = serde_json::to_string_pretty(&config_file)
            .map_err(|e| ConfigError::SerializeError(e.to_string()))?;

        fs::write(&self.config_path, content)
            .map_err(|e| ConfigError::WriteError(e.to_string()))?;

        Ok(())
    }

    /// Update settings and save to file
    pub fn update_settings(&mut self, settings: Settings) -> Result<(), ConfigError> {
        self.settings = settings;
        self.save()
    }

    /// Get current settings
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Get configuration file path
    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    /// Get configuration schema with available options
    pub fn schema() -> ConfigSchema {
        ConfigSchema {
            general: GeneralConfigSchema {
                target_fps: ConfigOption {
                    default: 60,
                    valid_range: Some((30, 120)),
                    valid_choices: None,
                    description: "UI refresh rate in frames per second".to_string(),
                    requires_restart: false,
                },
                enable_autosave: ConfigOption {
                    default: false,
                    valid_range: None,
                    valid_choices: None,
                    description: "Automatically save show files at regular intervals".to_string(),
                    requires_restart: false,
                },
                autosave_interval_secs: ConfigOption {
                    default: 300,
                    valid_range: Some((60, 3600)),
                    valid_choices: None,
                    description: "Autosave interval in seconds".to_string(),
                    requires_restart: false,
                },
            },
            audio: AudioConfigSchema {
                audio_device: ConfigOption {
                    default: "Default".to_string(),
                    valid_range: None,
                    valid_choices: None, // Will be populated from system enumeration
                    description: "Audio output device for playback".to_string(),
                    requires_restart: true,
                },
                audio_buffer_size: ConfigOption {
                    default: 512,
                    valid_range: None,
                    valid_choices: Some(vec![128, 256, 512, 1024, 2048]),
                    description: "Audio buffer size in samples".to_string(),
                    requires_restart: true,
                },
                audio_sample_rate: ConfigOption {
                    default: 48000,
                    valid_range: None,
                    valid_choices: Some(vec![44100, 48000, 96000]),
                    description: "Audio sample rate in Hz".to_string(),
                    requires_restart: true,
                },
            },
            midi: MidiConfigSchema {
                midi_enabled: ConfigOption {
                    default: false,
                    valid_range: None,
                    valid_choices: None,
                    description: "Enable MIDI input for live control".to_string(),
                    requires_restart: true,
                },
                midi_device: ConfigOption {
                    default: "None".to_string(),
                    valid_range: None,
                    valid_choices: None, // Will be populated from system enumeration
                    description: "MIDI input device".to_string(),
                    requires_restart: true,
                },
                midi_channel: ConfigOption {
                    default: 1,
                    valid_range: Some((1, 16)),
                    valid_choices: None,
                    description: "MIDI channel for input (1-16)".to_string(),
                    requires_restart: true,
                },
            },
            output: OutputConfigSchema {
                dmx_enabled: ConfigOption {
                    default: true,
                    valid_range: None,
                    valid_choices: None,
                    description: "Enable DMX output via Art-Net".to_string(),
                    requires_restart: true,
                },
                dmx_broadcast: ConfigOption {
                    default: false,
                    valid_range: None,
                    valid_choices: None,
                    description: "Use broadcast mode for Art-Net (vs unicast)".to_string(),
                    requires_restart: true,
                },
                dmx_source_ip: ConfigOption {
                    default: "192.168.1.100".to_string(),
                    valid_range: None,
                    valid_choices: None,
                    description: "Source IP address for Art-Net output".to_string(),
                    requires_restart: true,
                },
                dmx_dest_ip: ConfigOption {
                    default: "192.168.1.200".to_string(),
                    valid_range: None,
                    valid_choices: None,
                    description: "Destination IP address for Art-Net unicast".to_string(),
                    requires_restart: true,
                },
                dmx_port: ConfigOption {
                    default: 6454,
                    valid_range: Some((1024, 65535)),
                    valid_choices: None,
                    description: "UDP port for Art-Net output".to_string(),
                    requires_restart: true,
                },
                wled_enabled: ConfigOption {
                    default: false,
                    valid_range: None,
                    valid_choices: None,
                    description: "Enable WLED protocol support".to_string(),
                    requires_restart: true,
                },
                wled_ip: ConfigOption {
                    default: "192.168.1.50".to_string(),
                    valid_range: None,
                    valid_choices: None,
                    description: "IP address of WLED device".to_string(),
                    requires_restart: true,
                },
            },
            fixture: FixtureConfigSchema {
                enable_pan_tilt_limits: ConfigOption {
                    default: true,
                    valid_range: None,
                    valid_choices: None,
                    description: "Enable pan/tilt limiting for moving heads".to_string(),
                    requires_restart: false,
                },
            },
        }
    }

    /// Validate settings against schema
    pub fn validate_settings(settings: &Settings) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let schema = Self::schema();

        // Validate general settings
        if let Some((min, max)) = schema.general.target_fps.valid_range {
            if settings.target_fps < min || settings.target_fps > max {
                errors.push(format!("target_fps must be between {} and {}", min, max));
            }
        }

        if let Some((min, max)) = schema.general.autosave_interval_secs.valid_range {
            if settings.autosave_interval_secs < min || settings.autosave_interval_secs > max {
                errors.push(format!(
                    "autosave_interval_secs must be between {} and {}",
                    min, max
                ));
            }
        }

        // Validate audio settings
        if let Some(choices) = &schema.audio.audio_buffer_size.valid_choices {
            if !choices.contains(&settings.audio_buffer_size) {
                errors.push(format!("audio_buffer_size must be one of: {:?}", choices));
            }
        }

        if let Some(choices) = &schema.audio.audio_sample_rate.valid_choices {
            if !choices.contains(&settings.audio_sample_rate) {
                errors.push(format!("audio_sample_rate must be one of: {:?}", choices));
            }
        }

        // Validate MIDI settings
        if let Some((min, max)) = schema.midi.midi_channel.valid_range {
            if settings.midi_channel < min || settings.midi_channel > max {
                errors.push(format!("midi_channel must be between {} and {}", min, max));
            }
        }

        // Validate output settings
        if let Some((min, max)) = schema.output.dmx_port.valid_range {
            if settings.dmx_port < min || settings.dmx_port > max {
                errors.push(format!("dmx_port must be between {} and {}", min, max));
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    /// Reset settings to defaults
    pub fn reset_to_defaults(&mut self) -> Result<(), ConfigError> {
        self.settings = Settings::default();
        self.save()
    }
}

/// Configuration error types
#[derive(Debug)]
pub enum ConfigError {
    ReadError(String),
    WriteError(String),
    ParseError(String),
    SerializeError(String),
    ValidationError(Vec<String>),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::ReadError(msg) => write!(f, "Failed to read config file: {}", msg),
            ConfigError::WriteError(msg) => write!(f, "Failed to write config file: {}", msg),
            ConfigError::ParseError(msg) => write!(f, "Failed to parse config file: {}", msg),
            ConfigError::SerializeError(msg) => write!(f, "Failed to serialize config: {}", msg),
            ConfigError::ValidationError(errors) => {
                write!(f, "Config validation errors: {}", errors.join(", "))
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_config_manager_new() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.json");

        let manager = ConfigManager::new(Some(config_path.clone()));
        assert_eq!(manager.config_path(), config_path);
        assert_eq!(manager.settings(), &Settings::default());
    }

    #[test]
    fn test_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("test_config.json");

        let mut manager = ConfigManager::new(Some(config_path.clone()));

        // Modify settings
        let mut settings = Settings::default();
        settings.target_fps = 90;
        settings.audio_device = "Test Device".to_string();

        // Save settings
        manager.update_settings(settings.clone()).unwrap();

        // Load into new manager
        let mut manager2 = ConfigManager::new(Some(config_path));
        let loaded_settings = manager2.load().unwrap();

        assert_eq!(loaded_settings.target_fps, 90);
        assert_eq!(loaded_settings.audio_device, "Test Device");
    }

    #[test]
    fn test_validation() {
        let mut settings = Settings::default();

        // Valid settings should pass
        assert!(ConfigManager::validate_settings(&settings).is_ok());

        // Invalid settings should fail
        settings.target_fps = 200; // Outside valid range
        assert!(ConfigManager::validate_settings(&settings).is_err());

        settings.target_fps = 60; // Back to valid
        settings.midi_channel = 20; // Outside valid range
        assert!(ConfigManager::validate_settings(&settings).is_err());
    }

    #[test]
    fn test_schema_completeness() {
        let schema = ConfigManager::schema();

        // Ensure all settings have corresponding schema entries
        assert!(schema.general.target_fps.default > 0);
        assert!(!schema.audio.audio_device.description.is_empty());
        assert!(schema.midi.midi_channel.valid_range.is_some());
        assert!(schema.output.dmx_port.valid_range.is_some());
    }
}
