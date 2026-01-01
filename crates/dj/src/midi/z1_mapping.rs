//! TRAKTOR Kontrol Z1 MK1 MIDI mapping.
//!
//! The Z1 is a 2-channel mixer controller with:
//! - 2x Gain knobs
//! - 2x 3-band EQ (Hi/Mid/Lo)
//! - 2x Filter knobs
//! - 2x Volume faders
//! - 2x Cue buttons
//! - 2x FX buttons
//! - 1x Mode button
//! - 1x Crossfader
//!
//! In external mixer mode, the hardware EQ/filter/volume controls
//! are used directly on the mixer, so we can repurpose them in software.

use crate::deck::DeckId;
use crate::module::DjCommand;

/// TRAKTOR Kontrol Z1 MK1 MIDI CC and Note mappings.
///
/// Note: These values are based on the default Z1 MIDI mapping.
/// Actual values may vary and should be verified via MIDI learn.
pub struct Z1Mapping;

impl Z1Mapping {
    // Control Change (CC) numbers for knobs and faders
    pub const CC_GAIN_A: u8 = 16;
    pub const CC_GAIN_B: u8 = 17;
    pub const CC_EQ_HI_A: u8 = 18;
    pub const CC_EQ_MID_A: u8 = 19;
    pub const CC_EQ_LO_A: u8 = 20;
    pub const CC_EQ_HI_B: u8 = 21;
    pub const CC_EQ_MID_B: u8 = 22;
    pub const CC_EQ_LO_B: u8 = 23;
    pub const CC_FILTER_A: u8 = 24;
    pub const CC_FILTER_B: u8 = 25;
    pub const CC_VOLUME_A: u8 = 26;
    pub const CC_VOLUME_B: u8 = 27;
    pub const CC_CROSSFADER: u8 = 28;

    // Note numbers for buttons
    pub const NOTE_CUE_A: u8 = 1;
    pub const NOTE_CUE_B: u8 = 2;
    pub const NOTE_FX_A: u8 = 3;
    pub const NOTE_FX_B: u8 = 4;
    pub const NOTE_MODE: u8 = 5;

    /// Translate a MIDI note on message to a DJ command.
    pub fn translate_note_on(note: u8, _velocity: u8) -> Option<DjCommand> {
        match note {
            Self::NOTE_CUE_A => Some(DjCommand::CuePreview {
                deck: DeckId::A,
                pressed: true,
            }),
            Self::NOTE_CUE_B => Some(DjCommand::CuePreview {
                deck: DeckId::B,
                pressed: true,
            }),
            Self::NOTE_FX_A => Some(DjCommand::PlayPause { deck: DeckId::A }),
            Self::NOTE_FX_B => Some(DjCommand::PlayPause { deck: DeckId::B }),
            Self::NOTE_MODE => Some(DjCommand::ToggleSync { deck: DeckId::A }),
            _ => None,
        }
    }

    /// Translate a MIDI note off message to a DJ command.
    pub fn translate_note_off(note: u8) -> Option<DjCommand> {
        match note {
            Self::NOTE_CUE_A => Some(DjCommand::CuePreview {
                deck: DeckId::A,
                pressed: false,
            }),
            Self::NOTE_CUE_B => Some(DjCommand::CuePreview {
                deck: DeckId::B,
                pressed: false,
            }),
            _ => None,
        }
    }

    /// Translate a MIDI control change message to a DJ command.
    ///
    /// In external mixer mode, most CC messages go to hardware.
    /// We can optionally use some knobs for software control.
    pub fn translate_cc(cc: u8, value: u8) -> Option<DjCommand> {
        // Convert 0-127 MIDI value to 0.0-1.0 range
        let normalized = value as f64 / 127.0;

        match cc {
            // Filter knobs could be used for pitch/tempo in software
            Self::CC_FILTER_A => {
                // Map filter knob to pitch (-1.0 to 1.0)
                let pitch = (normalized * 2.0) - 1.0;
                Some(DjCommand::SetPitch {
                    deck: DeckId::A,
                    percent: pitch,
                })
            }
            Self::CC_FILTER_B => {
                let pitch = (normalized * 2.0) - 1.0;
                Some(DjCommand::SetPitch {
                    deck: DeckId::B,
                    percent: pitch,
                })
            }
            _ => None,
        }
    }

    /// Get the Z1 device name for MIDI port matching.
    pub fn device_name() -> &'static str {
        "Traktor Kontrol Z1"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_note_on() {
        let cmd = Z1Mapping::translate_note_on(Z1Mapping::NOTE_CUE_A, 127);
        assert!(matches!(
            cmd,
            Some(DjCommand::CuePreview {
                deck: DeckId::A,
                pressed: true
            })
        ));

        let cmd = Z1Mapping::translate_note_on(Z1Mapping::NOTE_FX_A, 127);
        assert!(matches!(
            cmd,
            Some(DjCommand::PlayPause { deck: DeckId::A })
        ));
    }

    #[test]
    fn test_translate_note_off() {
        let cmd = Z1Mapping::translate_note_off(Z1Mapping::NOTE_CUE_A);
        assert!(matches!(
            cmd,
            Some(DjCommand::CuePreview {
                deck: DeckId::A,
                pressed: false
            })
        ));
    }

    #[test]
    fn test_translate_cc() {
        // Center position (64) should be pitch 0
        let cmd = Z1Mapping::translate_cc(Z1Mapping::CC_FILTER_A, 64);
        if let Some(DjCommand::SetPitch { deck, percent }) = cmd {
            assert_eq!(deck, DeckId::A);
            assert!((percent - 0.0).abs() < 0.02);
        } else {
            panic!("Expected SetPitch command");
        }
    }
}
