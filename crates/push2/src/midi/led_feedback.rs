//! LED feedback for Push 2 pads.
//!
//! Manages the color state of Push 2 pads and generates MIDI messages
//! to update the LEDs.

use halo_dj::deck::DeckId;

/// Push 2 pad color palette indices.
///
/// The Push 2 uses a velocity-based color palette.
/// These are common colors from the palette.
pub mod colors {
    pub const OFF: u8 = 0;
    pub const WHITE: u8 = 3;
    pub const RED: u8 = 5;
    pub const RED_DIM: u8 = 6;
    pub const ORANGE: u8 = 9;
    pub const ORANGE_DIM: u8 = 10;
    pub const YELLOW: u8 = 13;
    pub const YELLOW_DIM: u8 = 14;
    pub const GREEN: u8 = 21;
    pub const GREEN_DIM: u8 = 22;
    pub const CYAN: u8 = 33;
    pub const CYAN_DIM: u8 = 34;
    pub const BLUE: u8 = 45;
    pub const BLUE_DIM: u8 = 46;
    pub const PURPLE: u8 = 49;
    pub const PURPLE_DIM: u8 = 50;
    pub const PINK: u8 = 57;
    pub const PINK_DIM: u8 = 58;
}

/// Color for a single pad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PadColor {
    /// Color palette index (velocity)
    pub color: u8,
    /// Animation mode (0=static, 1=blink, 2=pulse)
    pub animation: u8,
}

impl PadColor {
    pub const fn new(color: u8) -> Self {
        Self {
            color,
            animation: 0,
        }
    }

    pub const fn off() -> Self {
        Self::new(colors::OFF)
    }

    pub const fn with_animation(color: u8, animation: u8) -> Self {
        Self { color, animation }
    }
}

impl Default for PadColor {
    fn default() -> Self {
        Self::off()
    }
}

/// LED state for all Push 2 pads.
pub struct LedState {
    /// 8x8 grid of pad colors (64 pads, notes 36-99)
    pads: [[PadColor; 8]; 8],

    /// Dirty flags for each pad (needs update)
    dirty: [[bool; 8]; 8],
}

impl LedState {
    /// Create a new LED state with all pads off.
    pub fn new() -> Self {
        let mut state = Self {
            pads: [[PadColor::off(); 8]; 8],
            dirty: [[true; 8]; 8], // Mark all as dirty initially
        };

        // Set default colors for DJ row labels
        state.set_default_colors();

        state
    }

    /// Set default pad colors for the layout.
    fn set_default_colors(&mut self) {
        // Row 8 (Hot cues) - different colors per slot
        for i in 0..4 {
            // Deck A hot cues
            self.set_pad_color(7, i, PadColor::new(colors::RED_DIM));
            // Deck B hot cues
            self.set_pad_color(7, 4 + i, PadColor::new(colors::BLUE_DIM));
        }

        // Row 7 (Transport) - dim until active
        for i in 0..8 {
            self.set_pad_color(6, i, PadColor::new(colors::WHITE));
        }

        // Row 4-3 (Cue triggers) - blue dim
        for row in 4..6 {
            for col in 0..8 {
                self.set_pad_color(row, col, PadColor::new(colors::BLUE_DIM));
            }
        }

        // Row 2 (Fixtures) - cyan dim
        for col in 0..8 {
            self.set_pad_color(1, col, PadColor::new(colors::CYAN_DIM));
        }

        // Row 1 (Effects/Transport)
        self.set_pad_color(0, 4, PadColor::new(colors::GREEN)); // GO
        self.set_pad_color(0, 5, PadColor::new(colors::RED)); // STOP
        self.set_pad_color(0, 6, PadColor::new(colors::ORANGE)); // PREV
        self.set_pad_color(0, 7, PadColor::new(colors::ORANGE)); // NEXT
    }

    /// Set the color of a pad by row and column.
    pub fn set_pad_color(&mut self, row: usize, col: usize, color: PadColor) {
        if row < 8 && col < 8 {
            if self.pads[row][col] != color {
                self.pads[row][col] = color;
                self.dirty[row][col] = true;
            }
        }
    }

    /// Set pad color by MIDI note number.
    pub fn set_pad_color_by_note(&mut self, note: u8, color: PadColor) {
        if (36..=99).contains(&note) {
            let index = (note - 36) as usize;
            let row = index / 8;
            let col = index % 8;
            self.set_pad_color(row, col, color);
        }
    }

    /// Update hot cue LED based on state.
    pub fn update_hot_cue(&mut self, deck: DeckId, slot: u8, is_set: bool) {
        let col = match deck {
            DeckId::A => slot as usize,
            DeckId::B => 4 + slot as usize,
        };

        let color = if is_set {
            match slot {
                0 => PadColor::new(colors::RED),
                1 => PadColor::new(colors::GREEN),
                2 => PadColor::new(colors::BLUE),
                3 => PadColor::new(colors::YELLOW),
                _ => PadColor::new(colors::WHITE),
            }
        } else {
            match deck {
                DeckId::A => PadColor::new(colors::RED_DIM),
                DeckId::B => PadColor::new(colors::BLUE_DIM),
            }
        };

        self.set_pad_color(7, col, color);
    }

    /// Update transport LED based on playing state.
    pub fn update_transport(&mut self, deck: DeckId, is_playing: bool) {
        let col = match deck {
            DeckId::A => 1, // PLAY_A
            DeckId::B => 5, // PLAY_B
        };

        let color = if is_playing {
            PadColor::new(colors::GREEN)
        } else {
            PadColor::new(colors::GREEN_DIM)
        };

        self.set_pad_color(6, col, color);
    }

    /// Update sync LED.
    pub fn update_sync(&mut self, deck: DeckId, sync_enabled: bool) {
        let col = match deck {
            DeckId::A => 2, // SYNC_A
            DeckId::B => 6, // SYNC_B
        };

        let color = if sync_enabled {
            PadColor::new(colors::CYAN)
        } else {
            PadColor::new(colors::CYAN_DIM)
        };

        self.set_pad_color(6, col, color);
    }

    /// Update master LED.
    pub fn update_master(&mut self, deck: DeckId, is_master: bool) {
        let col = match deck {
            DeckId::A => 3, // MASTER_A
            DeckId::B => 7, // MASTER_B
        };

        let color = if is_master {
            PadColor::new(colors::ORANGE)
        } else {
            PadColor::new(colors::ORANGE_DIM)
        };

        self.set_pad_color(6, col, color);
    }

    /// Update cue trigger LED.
    pub fn update_cue_trigger(&mut self, cue_index: usize, is_active: bool) {
        if cue_index < 16 {
            let row = if cue_index < 8 { 5 } else { 4 };
            let col = cue_index % 8;

            let color = if is_active {
                PadColor::with_animation(colors::BLUE, 1) // Blink when active
            } else {
                PadColor::new(colors::BLUE_DIM)
            };

            self.set_pad_color(row, col, color);
        }
    }

    /// Update fixture selection LED.
    pub fn update_fixture_selection(&mut self, fixture_index: usize, is_selected: bool) {
        if fixture_index < 8 {
            let color = if is_selected {
                PadColor::new(colors::CYAN)
            } else {
                PadColor::new(colors::CYAN_DIM)
            };
            self.set_pad_color(1, fixture_index, color);
        }
    }

    /// Clear all LEDs.
    pub fn clear(&mut self) {
        for row in 0..8 {
            for col in 0..8 {
                self.set_pad_color(row, col, PadColor::off());
            }
        }
    }

    /// Generate MIDI messages for all dirty pads.
    pub fn to_midi_messages(&mut self) -> Vec<[u8; 3]> {
        let mut messages = Vec::new();

        for row in 0..8 {
            for col in 0..8 {
                if self.dirty[row][col] {
                    let note = 36 + (row * 8 + col) as u8;
                    let color = &self.pads[row][col];

                    // Note On message with velocity = color
                    messages.push([0x90, note, color.color]);

                    self.dirty[row][col] = false;
                }
            }
        }

        messages
    }

    /// Generate MIDI messages for all pads (full refresh).
    pub fn to_midi_messages_full(&self) -> Vec<[u8; 3]> {
        let mut messages = Vec::new();

        for row in 0..8 {
            for col in 0..8 {
                let note = 36 + (row * 8 + col) as u8;
                let color = &self.pads[row][col];
                messages.push([0x90, note, color.color]);
            }
        }

        messages
    }
}

impl Default for LedState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pad_note_calculation() {
        // Row 0, Col 0 = note 36
        // Row 7, Col 7 = note 99
        assert_eq!(36 + (0 * 8 + 0), 36);
        assert_eq!(36 + (7 * 8 + 7), 99);
    }

    #[test]
    fn test_led_state_update() {
        let mut state = LedState::new();

        state.update_transport(DeckId::A, true);
        let messages = state.to_midi_messages();

        // Should have at least one message for the play button
        assert!(!messages.is_empty());
    }
}
