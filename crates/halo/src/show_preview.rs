//! The Phase L3 role-based show lanes (look / energy / accent) and how
//! the UI draws the looks.
//!
//! The energy lane lives in `halo_light::energy`, is saved with the
//! track's cues, and scales the rig. The look lane lives in
//! `halo_light::looks` and drives the rig with its built-in looks, but is
//! still seeded per session, not saved. The accents are a UX preview: not
//! persisted, and not on the DMX path.

use eframe::egui::Color32;
use halo_light::cues::{CueSet, Lane};
use halo_light::energy::EnergyLane;
pub use halo_light::looks::{LookId, LookLane};

/// A mock look: name + signature color for the lane blocks and palette.
pub struct LookDef {
    pub name: &'static str,
    pub color: Color32,
}

/// Hardcoded palette the preview picks looks from. [`LookId`] indexes it.
pub const LOOK_PALETTE: [LookDef; 8] = [
    LookDef {
        name: "Warm Open",
        color: Color32::from_rgb(255, 170, 60),
    },
    LookDef {
        name: "Deep Blue",
        color: Color32::from_rgb(60, 110, 235),
    },
    LookDef {
        name: "Red Drop",
        color: Color32::from_rgb(230, 45, 45),
    },
    LookDef {
        name: "Strobe White",
        color: Color32::from_rgb(238, 238, 244),
    },
    LookDef {
        name: "Acid Green",
        color: Color32::from_rgb(120, 225, 70),
    },
    LookDef {
        name: "Magenta Chase",
        color: Color32::from_rgb(240, 60, 190),
    },
    LookDef {
        name: "Amber Sweep",
        color: Color32::from_rgb(250, 205, 40),
    },
    LookDef {
        name: "UV Violet",
        color: Color32::from_rgb(140, 70, 255),
    },
];

/// How the UI draws a look: its palette entry.
pub trait LookStyle {
    fn def(self) -> &'static LookDef;
}

impl LookStyle for LookId {
    fn def(self) -> &'static LookDef {
        &LOOK_PALETTE[self.0 % LOOK_PALETTE.len()]
    }
}

/// The accent lane borrows `CueSet`'s sorted/non-overlap machinery on one
/// designated legacy lane.
pub const ACCENT_LANE: Lane = Lane::Fx;

/// One track's show lanes: session-only looks and accents, and its saved
/// energy curve.
#[derive(Debug, Clone, Default)]
pub struct ShowPreview {
    pub looks: LookLane,
    pub energy: EnergyLane,
    /// Only [`ACCENT_LANE`] is populated.
    pub accents: CueSet,
}

/// Typed selection handle — the three lanes have independent id spaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShowSel {
    Look(u64),
    Energy(u64),
    Accent(u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palette_draws_the_rigs_looks() {
        assert_eq!(LOOK_PALETTE.len(), halo_light::looks::BUILTIN.len());
        for (i, def) in LOOK_PALETTE.iter().enumerate() {
            let look = halo_light::looks::builtin(LookId(i));
            assert_eq!(def.name, look.name);
            let [r, g, b] = look.swatch;
            assert_eq!(def.color, Color32::from_rgb(r, g, b), "{}", look.name);
        }
    }

    #[test]
    fn show_preview_default_is_empty() {
        let s = ShowPreview::default();
        assert!(s.looks.events().is_empty());
        assert!(s.energy.points().is_empty());
        assert!(
            s.accents
                .visible(ACCENT_LANE, f64::MIN, f64::MAX)
                .is_empty()
        );
    }
}
