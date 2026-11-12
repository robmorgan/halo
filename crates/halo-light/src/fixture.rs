//! Fixture rig model: the programmer's selection grid, with each grid
//! fixture patched to a real profile at a universe + start address, and
//! what the PATCH view checks it with: each universe's [`Occupancy`],
//! [`Rig::first_free`] addresses and [`Rig::conflicts`].
//! [`default_rig`] builds a plausible club rig from library profiles
//! with auto-assigned addresses.

use std::collections::{BTreeMap, HashMap};

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
    /// The profile's DMX mode it's patched in, by name; empty (a patch
    /// from before modes) is the profile's first.
    #[serde(default)]
    pub mode: String,
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

    /// What's patched on each universe, channel by channel.
    pub fn occupancy(&self, library: &FixtureLibrary) -> BTreeMap<u8, Occupancy> {
        let mut out: BTreeMap<u8, Occupancy> = BTreeMap::new();
        for f in &self.fixtures {
            let occ = out.entry(f.universe).or_default();
            if let Some(span) = f.span(library) {
                for c in span.start..=span.end {
                    let n = &mut occ.counts[usize::from(c) - 1];
                    *n = n.saturating_add(1);
                }
            }
        }
        out
    }

    /// The first address on `universe` with `footprint` free channels in
    /// a row, leaving `skip` (the fixture being moved) out; `None` when no
    /// run is long enough.
    pub fn first_free(
        &self,
        library: &FixtureLibrary,
        universe: u8,
        footprint: u16,
        skip: Option<u32>,
    ) -> Option<u16> {
        let mut taken = [false; 512];
        for f in &self.fixtures {
            if f.universe != universe || Some(f.id) == skip {
                continue;
            }
            if let Some(span) = f.span(library) {
                for c in span.start..=span.end {
                    taken[usize::from(c) - 1] = true;
                }
            }
        }
        let footprint = footprint.max(1);
        let mut run = 0u16;
        for (i, &t) in taken.iter().enumerate() {
            run = if t { 0 } else { run + 1 };
            if run >= footprint {
                return Some(i as u16 + 2 - footprint);
            }
        }
        None
    }

    /// The fixtures whose patch is invalid, and why: an unknown profile, a
    /// footprint running past channel 512, or channels shared with another
    /// fixture on the same universe. Every pair is compared, so an overlap
    /// is caught however far apart the two start.
    pub fn conflicts(&self, library: &FixtureLibrary) -> Conflicts {
        let mut by_id: HashMap<u32, Conflict> = HashMap::new();
        let mut spans: Vec<(&Fixture, Span)> = Vec::new();
        for f in &self.fixtures {
            let Some(span) = f.span(library) else {
                by_id.entry(f.id).or_default().unknown_profile = true;
                continue;
            };
            let past_end = span.past_end();
            if past_end > 0 {
                by_id.entry(f.id).or_default().past_end = past_end;
            }
            spans.push((f, span));
        }
        for (i, (a, sa)) in spans.iter().enumerate() {
            for (b, sb) in &spans[i + 1..] {
                if a.universe != b.universe {
                    continue;
                }
                let (first, last) = (sa.start.max(sb.start), sa.end.min(sb.end));
                if first > last {
                    continue;
                }
                for (me, other) in [(a, b), (b, a)] {
                    by_id.entry(me.id).or_default().overlaps.push(Overlap {
                        other: other.id,
                        first,
                        last,
                    });
                }
            }
        }
        Conflicts { by_id }
    }
}

/// A fixture's channels on its universe, 1-based and inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u16,
    /// The last channel, cut at 512.
    pub end: u16,
    /// The patched mode's channel count, which may run past `end`.
    pub footprint: u16,
}

impl Span {
    /// How many channels the footprint runs past 512.
    pub fn past_end(&self) -> u16 {
        (u32::from(self.start) + u32::from(self.footprint))
            .saturating_sub(513)
            .min(u32::from(u16::MAX)) as u16
    }
}

impl Fixture {
    /// Where the fixture sits on its universe; `None` for a profile the
    /// library doesn't know.
    pub fn span(&self, library: &FixtureLibrary) -> Option<Span> {
        let footprint = library.layout(self)?.footprint().max(1) as u16;
        let start = self.start_address.clamp(1, 512);
        let end = (u32::from(start) + u32::from(footprint) - 1).min(512) as u16;
        Some(Span {
            start,
            end,
            footprint,
        })
    }
}

/// How many fixtures patch each of a universe's 512 channels.
#[derive(Clone)]
pub struct Occupancy {
    counts: [u8; 512],
}

impl Default for Occupancy {
    fn default() -> Self {
        Occupancy { counts: [0; 512] }
    }
}

impl Occupancy {
    /// Fixtures on 1-based `channel`.
    pub fn count(&self, channel: u16) -> u8 {
        usize::from(channel)
            .checked_sub(1)
            .and_then(|i| self.counts.get(i))
            .copied()
            .unwrap_or(0)
    }

    /// Channels with at least one fixture.
    pub fn used(&self) -> u16 {
        self.counts.iter().filter(|&&n| n > 0).count() as u16
    }

    /// Runs of channels with more than one fixture, as (first, last).
    pub fn overlaps(&self) -> Vec<(u16, u16)> {
        let mut runs = Vec::new();
        let mut open: Option<u16> = None;
        for (i, &n) in self.counts.iter().enumerate() {
            let c = i as u16 + 1;
            match (n > 1, open) {
                (true, None) => open = Some(c),
                (false, Some(first)) => {
                    runs.push((first, c - 1));
                    open = None;
                }
                _ => {}
            }
        }
        if let Some(first) = open {
            runs.push((first, 512));
        }
        runs
    }
}

/// Channels one fixture shares with `other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Overlap {
    pub other: u32,
    pub first: u16,
    pub last: u16,
}

/// Why a fixture's patch is invalid.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Conflict {
    pub unknown_profile: bool,
    /// Channels past 512; 0 when it fits.
    pub past_end: u16,
    /// In rig order.
    pub overlaps: Vec<Overlap>,
}

impl Conflict {
    /// The channels it shares with any other fixture, first to last.
    pub fn range(&self) -> Option<(u16, u16)> {
        let first = self.overlaps.iter().map(|o| o.first).min()?;
        let last = self.overlaps.iter().map(|o| o.last).max()?;
        Some((first, last))
    }
}

/// Every conflicting fixture, by id.
#[derive(Debug, Clone, Default)]
pub struct Conflicts {
    by_id: HashMap<u32, Conflict>,
}

impl Conflicts {
    pub fn get(&self, id: u32) -> Option<&Conflict> {
        self.by_id.get(&id)
    }

    pub fn contains(&self, id: &u32) -> bool {
        self.by_id.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    pub fn ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.by_id.keys().copied()
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
            .and_then(|p| p.modes.first())
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
            mode: String::new(),
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
            let footprint = library.layout(f).expect("profile exists").footprint() as u16;
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
        let bad = rig.conflicts(&library);
        assert_eq!(bad.ids().collect::<HashSet<_>>(), HashSet::from([pb]));
        assert_eq!(bad.get(pb).unwrap().past_end, 400 + 192 - 513);

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

    #[test]
    fn a_patch_from_before_modes_loads_in_the_first_mode() {
        let library = FixtureLibrary::new();
        let json = r#"{"id":1,"kind":"Spot","label":"S1","col":0,"row":0,
            "profile_id":"shehds-led-spot-60w","universe":1,"start_address":1}"#;
        let f: Fixture = serde_json::from_str(json).expect("old patch loads");
        assert_eq!(f.mode, "");
        assert_eq!(library.layout(&f).unwrap().name, "9CH");
        // An unknown mode falls back the same way.
        let f = Fixture {
            mode: "42CH".into(),
            ..f
        };
        assert_eq!(library.layout(&f).unwrap().footprint(), 9);
        let f = Fixture {
            mode: "11CH".into(),
            ..f
        };
        assert_eq!(library.layout(&f).unwrap().footprint(), 11);
    }

    #[test]
    fn conflicts_use_the_patched_modes_footprint() {
        let library = FixtureLibrary::new();
        let mut rig = default_rig(&library);
        let spots: Vec<u32> = rig.ids_of_kind(FixtureKind::Spot).take(2).collect();
        for (f, addr) in rig
            .fixtures_mut()
            .iter_mut()
            .filter(|f| spots.contains(&f.id))
            .zip([401, 410])
        {
            f.universe = 9;
            f.start_address = addr;
        }
        // 9 channels from 401 end at 409: clear of 410.
        assert!(rig.conflicts(&library).is_empty());
        for f in rig.fixtures_mut().iter_mut().filter(|f| f.id == spots[0]) {
            f.mode = "11CH".into();
        }
        // 11 from 401 run to 411, over the second spot.
        let bad = rig.conflicts(&library);
        assert!(spots.iter().all(|id| bad.contains(id)), "{bad:?}");
        assert_eq!(bad.get(spots[1]).unwrap().range(), Some((410, 411)));
    }

    /// A fixture of `profile` at `universe`.`addr`, for the address tests.
    fn at(id: u32, profile: &str, universe: u8, start_address: u16) -> Fixture {
        Fixture {
            id,
            kind: FixtureKind::Par,
            label: format!("F{id}"),
            col: id as u8,
            row: 0,
            profile_id: profile.into(),
            mode: String::new(),
            universe,
            start_address,
        }
    }

    // 8 channels.
    const PAR: &str = "shehds-rgbw-par";
    // 192 channels.
    const BAR: &str = "clen-led-pixel-bar-64";

    #[test]
    fn occupancy_counts_each_universe_apart() {
        let library = FixtureLibrary::new();
        let rig = Rig::from_fixtures(vec![at(1, PAR, 1, 1), at(2, PAR, 1, 5), at(3, PAR, 2, 5)]);
        let occ = rig.occupancy(&library);
        assert_eq!(occ.keys().copied().collect::<Vec<_>>(), [1, 2]);
        // 1–8 and 5–12 share 5–8.
        assert_eq!(occ[&1].used(), 12);
        assert_eq!(occ[&1].overlaps(), [(5, 8)]);
        assert_eq!(
            (occ[&1].count(4), occ[&1].count(5), occ[&1].count(13)),
            (1, 2, 0)
        );
        assert_eq!(occ[&2].used(), 8);
        assert!(occ[&2].overlaps().is_empty());
        // Past 512, only the channels up to 512 are taken.
        let rig = Rig::from_fixtures(vec![at(1, PAR, 1, 509), at(2, PAR, 1, 510)]);
        let occ = rig.occupancy(&library);
        assert_eq!(occ[&1].used(), 4);
        assert_eq!(occ[&1].overlaps(), [(510, 512)]);
    }

    #[test]
    fn first_free_finds_the_first_gap_long_enough() {
        let library = FixtureLibrary::new();
        // 1–8, a 4-channel gap, then 13–20.
        let rig = Rig::from_fixtures(vec![at(1, PAR, 1, 1), at(2, PAR, 1, 13)]);
        assert_eq!(rig.first_free(&library, 1, 4, None), Some(9));
        assert_eq!(rig.first_free(&library, 1, 8, None), Some(21));
        // Leaving out the fixture being moved frees its own channels.
        assert_eq!(rig.first_free(&library, 1, 8, Some(1)), Some(1));
        // Another universe is empty.
        assert_eq!(rig.first_free(&library, 2, 8, None), Some(1));
        // Two bars and a PAR leave 120 channels: no room for a third bar.
        let rig = Rig::from_fixtures(vec![
            at(1, BAR, 3, 1),
            at(2, BAR, 3, 193),
            at(3, PAR, 3, 385),
        ]);
        assert_eq!(rig.first_free(&library, 3, 192, None), None);
        assert_eq!(rig.first_free(&library, 3, 120, None), Some(393));
    }

    #[test]
    fn conflicts_catch_overlaps_that_are_not_neighbours() {
        let library = FixtureLibrary::new();
        // A bar on 1–192, a PAR at 5–12 and one at 100–107: sorted by
        // start, the bar and the second PAR are never neighbours.
        let rig = Rig::from_fixtures(vec![
            at(1, BAR, 1, 1),
            at(2, PAR, 1, 5),
            at(3, PAR, 1, 100),
            at(4, PAR, 1, 300),
        ]);
        let bad = rig.conflicts(&library);
        assert_eq!(bad.len(), 3);
        assert!(!bad.contains(&4));
        let bar = bad.get(1).unwrap();
        assert_eq!(
            bar.overlaps,
            [
                Overlap {
                    other: 2,
                    first: 5,
                    last: 12
                },
                Overlap {
                    other: 3,
                    first: 100,
                    last: 107
                },
            ]
        );
        assert_eq!(bar.range(), Some((5, 107)));
        // Each side of a pair reports the same channels.
        assert_eq!(
            bad.get(3).unwrap().overlaps,
            [Overlap {
                other: 1,
                first: 100,
                last: 107
            }]
        );
        // Other universes don't clash.
        let rig = Rig::from_fixtures(vec![at(1, BAR, 1, 1), at(2, PAR, 2, 5)]);
        assert!(rig.conflicts(&library).is_empty());
    }

    #[test]
    fn a_fixture_past_512_still_overlaps_what_it_covers() {
        let library = FixtureLibrary::new();
        let rig = Rig::from_fixtures(vec![at(1, PAR, 1, 508), at(2, PAR, 1, 512)]);
        let bad = rig.conflicts(&library);
        let a = bad.get(1).unwrap();
        assert_eq!(a.past_end, 3);
        let b = bad.get(2).unwrap();
        assert_eq!(b.past_end, 7);
        // Both reach 512, and nothing past it counts.
        assert_eq!((a.range(), b.range()), (Some((512, 512)), Some((512, 512))));
    }
}
