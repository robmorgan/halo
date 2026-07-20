//! Fixture rig model: the programmer's selection grid, with each grid
//! fixture patched to a real profile at a universe + start address.
//! [`default_rig`] builds a plausible club rig from library profiles
//! with auto-assigned addresses; a patching UI will replace it.

use crate::cues::Lane;
use crate::fixture_library::FixtureLibrary;

/// sRGB color, UI-toolkit-free; the egui layer converts at the edge.
pub type Rgb = [u8; 3];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum FixtureKind {
    Spot,
    Wash,
    Par,
    Strobe,
    Smoke,
    Pyro,
    PixelBar,
}

pub const ALL_KINDS: [FixtureKind; 7] = [
    FixtureKind::Spot,
    FixtureKind::Wash,
    FixtureKind::Par,
    FixtureKind::Strobe,
    FixtureKind::Smoke,
    FixtureKind::Pyro,
    FixtureKind::PixelBar,
];

impl FixtureKind {
    /// Which trigger lane drives this kind.
    pub fn lane(self) -> Lane {
        match self {
            Self::Spot | Self::Wash | Self::Par | Self::Strobe => Lane::Lighting,
            Self::PixelBar => Lane::Pixels,
            Self::Smoke | Self::Pyro => Lane::Fx,
        }
    }

    /// Group-select button label.
    pub fn group_label(self) -> &'static str {
        match self {
            Self::Spot => "SPOTS",
            Self::Wash => "WASHES",
            Self::Par => "PARS",
            Self::Strobe => "STROBES",
            Self::Smoke => "SMOKE",
            Self::Pyro => "PYRO",
            Self::PixelBar => "PIXELS",
        }
    }

    /// Library profile backing this kind in the default rig.
    pub fn default_profile_id(self) -> &'static str {
        match self {
            Self::Spot => "shehds-led-spot-60w",
            Self::Wash => "shehds-led-wash-7x18w-rgbwa-uv",
            Self::Par => "shehds-rgbw-par",
            Self::Strobe => "hyulights-led-rgbw-4in1-48-partition-strobe",
            Self::Smoke => "dl-geyser-1000-led-smoke-machine-1000w-3x9w-rgb",
            Self::Pyro => "generic-pyro-igniter",
            Self::PixelBar => "clen-led-pixel-bar-64",
        }
    }

    /// Cell-label prefix ("S1", "PB3", …).
    pub fn short(self) -> &'static str {
        match self {
            Self::Spot => "S",
            Self::Wash => "W",
            Self::Par => "P",
            Self::Strobe => "ST",
            Self::Smoke => "SM",
            Self::Pyro => "PY",
            Self::PixelBar => "PB",
        }
    }

    /// Family hue for grid cells: the lighting kinds stay in the blue
    /// family of the Lighting lane, pixel bars take the lane pink, and
    /// smoke/pyro read literally (grey smoke, flame orange).
    pub fn color(self) -> Rgb {
        match self {
            Self::Spot => [80, 165, 255],
            Self::Wash => [140, 120, 255],
            Self::Par => [70, 130, 215],
            Self::Strobe => [225, 235, 250],
            Self::Smoke => [150, 160, 172],
            Self::Pyro => [255, 115, 60],
            Self::PixelBar => [240, 95, 175],
        }
    }
}

/// One patched fixture: positioned on the selection grid so the layout
/// mirrors the physical rig, and addressed on the wire via its profile.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Fixture {
    pub id: u32,
    pub kind: FixtureKind,
    pub label: String,
    pub col: u8,
    pub row: u8,
    /// Key into the [`FixtureLibrary`].
    pub profile_id: String,
    pub universe: u8,
    /// 1-based DMX start address.
    pub start_address: u16,
}

#[derive(Clone)]
pub struct Rig {
    fixtures: Vec<Fixture>,
}

impl Rig {
    pub fn iter(&self) -> impl Iterator<Item = &Fixture> {
        self.fixtures.iter()
    }

    pub fn ids_of_kind(&self, kind: FixtureKind) -> impl Iterator<Item = u32> + '_ {
        self.fixtures
            .iter()
            .filter(move |f| f.kind == kind)
            .map(|f| f.id)
    }

    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.fixtures.iter().map(|f| f.id)
    }

    /// Grid extent as (cols, rows).
    pub fn extent(&self) -> (u8, u8) {
        let cols = self.fixtures.iter().map(|f| f.col + 1).max().unwrap_or(0);
        let rows = self.fixtures.iter().map(|f| f.row + 1).max().unwrap_or(0);
        (cols, rows)
    }

    pub fn from_fixtures(fixtures: Vec<Fixture>) -> Self {
        Rig { fixtures }
    }

    /// Direct access for the patch editor.
    pub fn fixtures_mut(&mut self) -> &mut Vec<Fixture> {
        &mut self.fixtures
    }

    pub fn next_id(&self) -> u32 {
        self.fixtures.iter().map(|f| f.id).max().unwrap_or(0) + 1
    }

    /// Ids of fixtures whose patch is invalid: unknown profile, footprint
    /// spilling past channel 512, or overlapping another fixture on the
    /// same universe.
    pub fn conflicts(&self, library: &FixtureLibrary) -> std::collections::HashSet<u32> {
        let mut bad = std::collections::HashSet::new();
        // (universe, start, end-inclusive, id) for overlap sweeping.
        let mut spans: Vec<(u8, u16, u16, u32)> = Vec::new();
        for f in &self.fixtures {
            let Some(profile) = library.get(&f.profile_id) else {
                bad.insert(f.id);
                continue;
            };
            let fp = profile.footprint() as u16;
            if f.start_address < 1 || u32::from(f.start_address) + u32::from(fp) - 1 > 512 {
                bad.insert(f.id);
                continue;
            }
            spans.push((f.universe, f.start_address, f.start_address + fp - 1, f.id));
        }
        spans.sort_unstable();
        for pair in spans.windows(2) {
            let (u_a, _, end_a, id_a) = pair[0];
            let (u_b, start_b, _, id_b) = pair[1];
            if u_a == u_b && start_b <= end_a {
                bad.insert(id_a);
                bad.insert(id_b);
            }
        }
        bad
    }
}

/// Persisted patch form (JSON in the library DB), mirroring `CueFile`.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct RigFile {
    pub version: u32,
    pub fixtures: Vec<Fixture>,
}

impl RigFile {
    pub fn from_rig(rig: &Rig) -> Self {
        RigFile {
            version: 1,
            fixtures: rig.fixtures.clone(),
        }
    }

    pub fn into_rig(self) -> Rig {
        Rig {
            fixtures: self.fixtures,
        }
    }
}

/// Default patch, arranged like a stage (top row = truss, bottom =
/// floor), addressed from real library profiles:
///
/// ```text
/// row 0  S1 S2 W1 W2 W3 W4 S3 S4      4 spots flanking 4 washes
/// row 1  P1 P2 P3 P4 P5 P6 ST1 ST2    6 PARs + 2 strobes
/// row 2  PB1 ..                PB8    8 pixel bars
/// row 3  SM1 SM2           PY1 PY2    smoke + pyro on the floor
/// ```
///
/// Conventionals pack sequentially into universe 1; the 192-channel
/// pixel bars go two per universe starting at universe 2.
pub fn default_rig(library: &FixtureLibrary) -> Rig {
    let mut fixtures = Vec::new();
    let mut next_id = 0u32;
    let mut counts = std::collections::HashMap::new();
    let mut next_addr_u1: u16 = 1;
    let mut pixel_bars_placed: u8 = 0;
    let mut place = |kind: FixtureKind, col: u8, row: u8| {
        let n = counts.entry(kind).or_insert(0u32);
        *n += 1;
        let profile_id = kind.default_profile_id();
        let footprint = library
            .get(profile_id)
            .expect("default rig uses a library profile")
            .footprint() as u16;
        let (universe, start_address) = if kind == FixtureKind::PixelBar {
            let universe = 2 + pixel_bars_placed / 2;
            let addr = 1 + (pixel_bars_placed % 2) as u16 * footprint;
            pixel_bars_placed += 1;
            (universe, addr)
        } else {
            let addr = next_addr_u1;
            next_addr_u1 += footprint;
            (1, addr)
        };
        fixtures.push(Fixture {
            id: {
                next_id += 1;
                next_id
            },
            kind,
            label: format!("{}{n}", kind.short()),
            col,
            row,
            profile_id: profile_id.to_string(),
            universe,
            start_address,
        });
    };

    use FixtureKind::*;
    for (col, kind) in [Spot, Spot, Wash, Wash, Wash, Wash, Spot, Spot]
        .into_iter()
        .enumerate()
    {
        place(kind, col as u8, 0);
    }
    for col in 0..6 {
        place(Par, col, 1);
    }
    place(Strobe, 6, 1);
    place(Strobe, 7, 1);
    for col in 0..8 {
        place(PixelBar, col, 2);
    }
    place(Smoke, 0, 3);
    place(Smoke, 1, 3);
    place(Pyro, 6, 3);
    place(Pyro, 7, 3);

    Rig { fixtures }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn default_rig_is_well_formed() {
        let rig = default_rig(&FixtureLibrary::new());
        let ids: HashSet<u32> = rig.ids().collect();
        assert_eq!(ids.len(), rig.iter().count(), "ids must be unique");
        let cells: HashSet<(u8, u8)> = rig.iter().map(|f| (f.col, f.row)).collect();
        assert_eq!(cells.len(), rig.iter().count(), "grid cells must be unique");
        assert_eq!(rig.ids_of_kind(FixtureKind::Spot).count(), 4);
        assert_eq!(rig.ids_of_kind(FixtureKind::Wash).count(), 4);
        assert_eq!(rig.ids_of_kind(FixtureKind::Par).count(), 6);
        assert_eq!(rig.ids_of_kind(FixtureKind::Strobe).count(), 2);
        assert_eq!(rig.ids_of_kind(FixtureKind::PixelBar).count(), 8);
        assert_eq!(rig.ids_of_kind(FixtureKind::Smoke).count(), 2);
        assert_eq!(rig.ids_of_kind(FixtureKind::Pyro).count(), 2);
        assert_eq!(rig.extent(), (8, 4));
    }

    #[test]
    fn default_rig_patch_is_valid_dmx() {
        let library = FixtureLibrary::new();
        let rig = default_rig(&library);
        // Every fixture's footprint fits its universe, and no two
        // fixtures overlap on the wire.
        let mut occupied: HashSet<(u8, u16)> = HashSet::new();
        for f in rig.iter() {
            let profile = library.get(&f.profile_id).expect("profile exists");
            let footprint = profile.footprint() as u16;
            assert!(f.start_address >= 1, "{}: addresses are 1-based", f.label);
            assert!(
                f.start_address + footprint - 1 <= 512,
                "{}: footprint spills past the universe",
                f.label
            );
            for addr in f.start_address..f.start_address + footprint {
                assert!(
                    occupied.insert((f.universe, addr)),
                    "{}: address {}:{} double-patched",
                    f.label,
                    f.universe,
                    addr
                );
            }
        }
    }

    #[test]
    fn rig_roundtrips_through_rigfile_json() {
        let library = FixtureLibrary::new();
        let rig = default_rig(&library);
        let json = serde_json::to_string(&RigFile::from_rig(&rig)).unwrap();
        let parsed: RigFile = serde_json::from_str(&json).unwrap();
        let restored = parsed.into_rig();
        assert_eq!(restored.iter().count(), rig.iter().count());
        let f = restored.iter().find(|f| f.label == "PB3").unwrap();
        assert_eq!(f.profile_id, "clen-led-pixel-bar-64");
        assert_eq!((f.universe, f.start_address), (3, 1));
    }

    #[test]
    fn conflicts_flag_overlap_spill_and_unknown_profile() {
        let library = FixtureLibrary::new();
        let mut rig = default_rig(&library);
        assert!(rig.conflicts(&library).is_empty(), "default patch is clean");

        // P2 moved onto P1's footprint: both flagged.
        let (p1, p2) = {
            let a = rig.iter().find(|f| f.label == "P1").unwrap();
            let b = rig.iter().find(|f| f.label == "P2").unwrap();
            (a.id, b.id)
        };
        let p1_addr = rig.iter().find(|f| f.id == p1).unwrap().start_address;
        rig.fixtures_mut()
            .iter_mut()
            .find(|f| f.id == p2)
            .unwrap()
            .start_address = p1_addr + 1;
        let bad = rig.conflicts(&library);
        assert!(bad.contains(&p1) && bad.contains(&p2));

        // A pixel bar pushed past channel 512: flagged alone.
        let mut rig = default_rig(&library);
        let pb = rig.iter().find(|f| f.label == "PB1").unwrap().id;
        rig.fixtures_mut()
            .iter_mut()
            .find(|f| f.id == pb)
            .unwrap()
            .start_address = 400;
        assert_eq!(rig.conflicts(&library), HashSet::from([pb]));

        // Unknown profile: flagged.
        let mut rig = default_rig(&library);
        let s1 = rig.iter().find(|f| f.label == "S1").unwrap().id;
        rig.fixtures_mut()
            .iter_mut()
            .find(|f| f.id == s1)
            .unwrap()
            .profile_id = "no-such-profile".to_string();
        assert!(rig.conflicts(&library).contains(&s1));
    }

    #[test]
    fn every_kind_maps_to_a_lane_and_group() {
        for kind in ALL_KINDS {
            let _ = kind.lane();
            assert!(!kind.group_label().is_empty());
            assert!(!kind.short().is_empty());
        }
    }
}
