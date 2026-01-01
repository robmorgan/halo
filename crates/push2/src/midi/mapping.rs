//! Push 2 MIDI mapping.
//!
//! Maps Push 2 pads, encoders, and buttons to DJ and lighting commands.
//!
//! # Pad Layout (8x8 grid, notes 36-99)
//!
//! ```text
//! DJ (Top Half - notes 68-99):
//! Row 8 (92-99): Hot Cues A1-4, B1-4
//! Row 7 (84-91): CUE, PLAY, SYNC, MASTER for each deck
//! Row 6 (76-83): Seek, Loop, Load for each deck
//! Row 5 (68-75): Global DJ controls
//!
//! Lighting (Bottom Half - notes 36-67):
//! Row 4 (60-67): Cue triggers 1-8
//! Row 3 (52-59): Cue triggers 9-16
//! Row 2 (44-51): Fixture selection
//! Row 1 (36-43): Effects, GO, STOP, PREV, NEXT
//! ```

use halo_core::ConsoleCommand;

/// Push 2 MIDI mapping constants and translation.
pub struct Push2Mapping;

impl Push2Mapping {
    // === Pad Notes (8x8 grid) ===

    // Row 8: Hot Cues (notes 92-99)
    pub const HOT_CUE_A_1: u8 = 92;
    pub const HOT_CUE_A_2: u8 = 93;
    pub const HOT_CUE_A_3: u8 = 94;
    pub const HOT_CUE_A_4: u8 = 95;
    pub const HOT_CUE_B_1: u8 = 96;
    pub const HOT_CUE_B_2: u8 = 97;
    pub const HOT_CUE_B_3: u8 = 98;
    pub const HOT_CUE_B_4: u8 = 99;

    // Row 7: Transport (notes 84-91)
    pub const CUE_A: u8 = 84;
    pub const PLAY_A: u8 = 85;
    pub const SYNC_A: u8 = 86;
    pub const MASTER_A: u8 = 87;
    pub const CUE_B: u8 = 88;
    pub const PLAY_B: u8 = 89;
    pub const SYNC_B: u8 = 90;
    pub const MASTER_B: u8 = 91;

    // Row 6: Navigation (notes 76-83)
    pub const SEEK_BACK_A: u8 = 76;
    pub const SEEK_FWD_A: u8 = 77;
    pub const LOOP_A: u8 = 78;
    pub const LOAD_A: u8 = 79;
    pub const SEEK_BACK_B: u8 = 80;
    pub const SEEK_FWD_B: u8 = 81;
    pub const LOOP_B: u8 = 82;
    pub const LOAD_B: u8 = 83;

    // Row 5: Global DJ (notes 68-75)
    pub const TEMPO_RANGE: u8 = 68;
    pub const ABLETON_LINK: u8 = 69;
    pub const TAP_TEMPO: u8 = 70;
    pub const _DJ_MODE: u8 = 71;
    pub const _BPM_MINUS: u8 = 72;
    pub const _BPM_PLUS: u8 = 73;
    pub const _RESERVED_1: u8 = 74;
    pub const BUTTON_SHIFT: u8 = 75;

    // Row 4: Cue triggers 1-8 (notes 60-67)
    // Row 3: Cue triggers 9-16 (notes 52-59)
    // Row 2: Fixture selection (notes 44-51)
    // Row 1: Effects/transport (notes 36-43)
    pub const GO: u8 = 40;
    pub const STOP: u8 = 41;
    pub const PREV_CUE: u8 = 42;
    pub const NEXT_CUE_LIST: u8 = 43;

    // === Encoders (CC 71-78) ===
    pub const ENCODER_1: u8 = 71;
    pub const ENCODER_2: u8 = 72;
    pub const ENCODER_3: u8 = 73;
    pub const ENCODER_4: u8 = 74;
    pub const ENCODER_5: u8 = 75;
    pub const ENCODER_6: u8 = 76;
    pub const ENCODER_7: u8 = 77;
    pub const ENCODER_8: u8 = 78;

    // Touch strip
    pub const TOUCH_STRIP: u8 = 12;

    /// Translate a MIDI note on message to a console command.
    pub fn translate_note_on(note: u8, velocity: u8, shift_held: bool) -> Option<ConsoleCommand> {
        // Hot Cues (Row 8)
        match note {
            Self::HOT_CUE_A_1..=Self::HOT_CUE_A_4 => {
                let slot = note - Self::HOT_CUE_A_1;
                if shift_held {
                    return Some(ConsoleCommand::DjSetHotCue { deck: 0, slot });
                } else {
                    return Some(ConsoleCommand::DjJumpToHotCue { deck: 0, slot });
                }
            }
            Self::HOT_CUE_B_1..=Self::HOT_CUE_B_4 => {
                let slot = note - Self::HOT_CUE_B_1;
                if shift_held {
                    return Some(ConsoleCommand::DjSetHotCue { deck: 1, slot });
                } else {
                    return Some(ConsoleCommand::DjJumpToHotCue { deck: 1, slot });
                }
            }
            _ => {}
        }

        // Transport (Row 7)
        match note {
            Self::CUE_A => {
                return Some(ConsoleCommand::DjCuePreview {
                    deck: 0,
                    pressed: true,
                });
            }
            Self::PLAY_A => return Some(ConsoleCommand::DjPlay { deck: 0 }),
            Self::SYNC_A => return Some(ConsoleCommand::DjToggleSync { deck: 0 }),
            Self::MASTER_A => return Some(ConsoleCommand::DjSetMaster { deck: 0 }),

            Self::CUE_B => {
                return Some(ConsoleCommand::DjCuePreview {
                    deck: 1,
                    pressed: true,
                });
            }
            Self::PLAY_B => return Some(ConsoleCommand::DjPlay { deck: 1 }),
            Self::SYNC_B => return Some(ConsoleCommand::DjToggleSync { deck: 1 }),
            Self::MASTER_B => return Some(ConsoleCommand::DjSetMaster { deck: 1 }),
            _ => {}
        }

        // Navigation (Row 6)
        match note {
            Self::SEEK_BACK_A => {
                return Some(ConsoleCommand::DjSeekBeats { deck: 0, beats: -4 });
            }
            Self::SEEK_FWD_A => {
                return Some(ConsoleCommand::DjSeekBeats { deck: 0, beats: 4 });
            }
            Self::SEEK_BACK_B => {
                return Some(ConsoleCommand::DjSeekBeats { deck: 1, beats: -4 });
            }
            Self::SEEK_FWD_B => {
                return Some(ConsoleCommand::DjSeekBeats { deck: 1, beats: 4 });
            }
            _ => {}
        }

        // Global DJ (Row 5)
        match note {
            Self::TAP_TEMPO => return Some(ConsoleCommand::TapTempo),
            Self::ABLETON_LINK => return Some(ConsoleCommand::ToggleAbletonLink),
            _ => {}
        }

        None
    }

    /// Translate a MIDI note off message to a console command.
    pub fn translate_note_off(note: u8) -> Option<ConsoleCommand> {
        // Release cue preview
        match note {
            Self::CUE_A => {
                return Some(ConsoleCommand::DjCuePreview {
                    deck: 0,
                    pressed: false,
                });
            }
            Self::CUE_B => {
                return Some(ConsoleCommand::DjCuePreview {
                    deck: 1,
                    pressed: false,
                });
            }
            _ => {}
        }

        None
    }

    /// Translate a MIDI control change message to a console command.
    ///
    /// Encoders send relative values: 64 = no change, <64 = decrease, >64 = increase.
    pub fn translate_cc(cc: u8, value: u8) -> Option<ConsoleCommand> {
        // Calculate relative delta (-63 to +63)
        let delta = value as i8 - 64;
        if delta == 0 {
            return None;
        }

        match cc {
            // Encoder 1: Deck A pitch
            Self::ENCODER_1 => {
                // Each tick = 0.5% pitch change
                let pitch_delta = delta as f64 * 0.005;
                Some(ConsoleCommand::DjNudgePitch {
                    deck: 0,
                    delta: pitch_delta,
                })
            }

            // Encoder 5: Deck B pitch
            Self::ENCODER_5 => {
                let pitch_delta = delta as f64 * 0.005;
                Some(ConsoleCommand::DjNudgePitch {
                    deck: 1,
                    delta: pitch_delta,
                })
            }

            // Touch strip: scrub/seek
            Self::TOUCH_STRIP => {
                // Absolute position 0-127 maps to track position
                let position = value as f64 / 127.0;
                // This would need the current track duration to calculate seconds
                // For now, we'll send a normalized position
                None // TODO: Implement scrub with track duration context
            }

            _ => None,
        }
    }

    /// Get the Push 2 device name for MIDI port matching.
    pub fn device_name() -> &'static str {
        "Ableton Push 2"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hot_cue_mapping() {
        let cmd = Push2Mapping::translate_note_on(Push2Mapping::HOT_CUE_A_1, 127, false);
        assert!(matches!(
            cmd,
            Some(ConsoleCommand::DjJumpToHotCue { deck: 0, slot: 0 })
        ));

        // With shift = set hot cue
        let cmd = Push2Mapping::translate_note_on(Push2Mapping::HOT_CUE_A_1, 127, true);
        assert!(matches!(
            cmd,
            Some(ConsoleCommand::DjSetHotCue { deck: 0, slot: 0 })
        ));
    }

    #[test]
    fn test_transport_mapping() {
        let cmd = Push2Mapping::translate_note_on(Push2Mapping::PLAY_A, 127, false);
        assert!(matches!(cmd, Some(ConsoleCommand::DjPlay { deck: 0 })));

        let cmd = Push2Mapping::translate_note_on(Push2Mapping::SYNC_B, 127, false);
        assert!(matches!(
            cmd,
            Some(ConsoleCommand::DjToggleSync { deck: 1 })
        ));
    }

    #[test]
    fn test_cue_preview() {
        // Note on = press
        let cmd = Push2Mapping::translate_note_on(Push2Mapping::CUE_A, 127, false);
        assert!(matches!(
            cmd,
            Some(ConsoleCommand::DjCuePreview {
                deck: 0,
                pressed: true
            })
        ));

        // Note off = release
        let cmd = Push2Mapping::translate_note_off(Push2Mapping::CUE_A);
        assert!(matches!(
            cmd,
            Some(ConsoleCommand::DjCuePreview {
                deck: 0,
                pressed: false
            })
        ));
    }
}
