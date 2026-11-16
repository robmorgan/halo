//! Fixture rig model: each fixture patched to a real profile at a
//! universe + start address, its cell on the stage [`layout`] the
//! programmer's selection grid mirrors, and what the PATCH view checks
//! the patch with: each universe's [`Occupancy`], [`Rig::first_free`]
//! addresses and [`Rig::conflicts`]. [`default_rig`] builds a plausible
//! club rig from library profiles with auto-assigned addresses.

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

/// A cell on the stage grid, column and row from upstage left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Cell {
    pub col: u8,
    pub row: u8,
}

impl Cell {
    pub const fn new(col: u8, row: u8) -> Self {
        Cell { col, row }
    }
}

/// The stage grid: 8 columns, 8 rows, upstage at the top.
pub const STAGE_COLS: u8 = 8;
pub const STAGE_ROWS: u8 = 8;

/// One patched fixture: addressed on the wire via its profile, and on
/// the stage grid where it's placed by hand, or by [`layout`].
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Fixture {
    pub id: u32,
    pub kind: FixtureKind,
    pub label: String,
    /// Its cell on the stage, placed by hand; `None` is arranged
    /// automatically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pos: Option<Cell>,
    /// Key into the [`FixtureLibrary`].
    pub profile_id: String,
    /// The profile's DMX mode it's patched in, by name; empty (a patch
    /// from before modes) is the profile's first.
    #[serde(default)]
    pub mode: String,
    pub universe: u8,
    /// 1-based DMX start address.
    pub start_address: u16,
    /// Unpatched: kept in the rig and on the programmer's grid, with its
    /// universe and address remembered, but sent no DMX and left out of
    /// the patch checks.
    #[serde(default)]
    pub unpatched: bool,
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

    /// The fixtures on the wire: all but the unpatched.
    pub fn patched(&self) -> impl Iterator<Item = &Fixture> {
        self.fixtures.iter().filter(|f| !f.unpatched)
    }

    /// The programmer grid's extent as (cols, rows): the stage's width,
    /// so centred rows stay centred, down to its last used row.
    pub fn extent(&self) -> (u8, u8) {
        let rows = layout(self).values().map(|c| c.row + 1).max().unwrap_or(0);
        (if rows == 0 { 0 } else { STAGE_COLS }, rows)
    }

    /// How many fixtures are placed by hand.
    pub fn placed(&self) -> usize {
        self.fixtures.iter().filter(|f| f.pos.is_some()).count()
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
        for f in self.patched() {
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
        for f in self.patched() {
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

    /// The patched fixtures whose patch is invalid, and why: an unknown profile, a
    /// footprint running past channel 512, or channels shared with another
    /// fixture on the same universe. Every pair is compared, so an overlap
    /// is caught however far apart the two start.
    pub fn conflicts(&self, library: &FixtureLibrary) -> Conflicts {
        let mut by_id: HashMap<u32, Conflict> = HashMap::new();
        let mut spans: Vec<(&Fixture, Span)> = Vec::new();
        for f in self.patched() {
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

/// Every fixture's cell on the stage, by id. Hand-placed fixtures keep
/// theirs. The rest are arranged by kind, in [`ALL_KINDS`] order: kinds
/// share a row while their fixtures fit in it, a cell apart when there's
/// room, and a kind too big for a row gets rows of its own. Each row is
/// centred on its kinds' whole counts (so a fixture placed elsewhere
/// leaves its gap rather than shifting the others), in patch order, each
/// fixture skipping a taken cell for the next free one, row by row,
/// wrapping. More fixtures than cells share the last one.
pub fn layout(rig: &Rig) -> HashMap<u32, Cell> {
    let cols = usize::from(STAGE_COLS);
    let total = cols * usize::from(STAGE_ROWS);
    let mut taken = vec![false; total];
    let mut out = HashMap::new();
    let index = |c: Cell| usize::from(c.row) * cols + usize::from(c.col);
    for f in &rig.fixtures {
        if let Some(c) = f.pos.filter(|c| c.col < STAGE_COLS && c.row < STAGE_ROWS)
            && !taken[index(c)]
        {
            taken[index(c)] = true;
            out.insert(f.id, c);
        }
    }

    // The kinds present, grouped into rows while they fit.
    let mut rows: Vec<Vec<Vec<&Fixture>>> = Vec::new();
    for kind in ALL_KINDS {
        let of_kind: Vec<&Fixture> = rig.fixtures.iter().filter(|f| f.kind == kind).collect();
        if of_kind.is_empty() {
            continue;
        }
        match rows.last_mut() {
            Some(row) if row.iter().map(Vec::len).sum::<usize>() + of_kind.len() <= cols => {
                row.push(of_kind);
            }
            _ => rows.push(vec![of_kind]),
        }
    }

    let mut row = 0;
    for kinds in rows {
        let count: usize = kinds.iter().map(Vec::len).sum();
        let gap = usize::from(count + kinds.len() - 1 <= cols);
        let width = count + gap * (kinds.len() - 1);
        let mut col = cols.saturating_sub(width) / 2;
        for of_kind in &kinds {
            for (i, f) in of_kind.iter().enumerate() {
                if out.contains_key(&f.id) {
                    continue;
                }
                let first = (row * cols + col + i) % total;
                let n = (0..total)
                    .map(|k| (first + k) % total)
                    .find(|&n| !taken[n])
                    .unwrap_or(total - 1);
                taken[n] = true;
                out.insert(f.id, Cell::new((n % cols) as u8, (n / cols) as u8));
            }
            col += of_kind.len() + gap;
        }
        row += width.div_ceil(cols);
    }
    out
}

/// Persisted patch form (JSON in the library DB), mirroring `CueFile`.
/// Version 2 places fixtures on the stage by `pos`; a version 1 file
/// loads with every fixture arranged automatically.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct RigFile {
    pub version: u32,
    pub fixtures: Vec<Fixture>,
}

impl RigFile {
    pub fn from_rig(rig: &Rig) -> Self {
        RigFile {
            version: 2,
            fixtures: rig.fixtures.clone(),
        }
    }

    pub fn into_rig(self) -> Rig {
        Rig {
            fixtures: self.fixtures,
        }
    }
}

/// Default patch from real library profiles: 4 spots, 4 washes, 6
/// PARs, 2 strobes, 8 pixel bars, 2 smoke machines and 2 pyro, all
/// arranged automatically on the stage. Conventionals pack sequentially
/// into universe 1 (spots either side of the washes); the 192-channel
/// pixel bars go two per universe starting at universe 2.
pub fn default_rig(library: &FixtureLibrary) -> Rig {
    let mut fixtures = Vec::new();
    let mut next_id = 0u32;
    let mut counts = std::collections::HashMap::new();
    let mut next_addr_u1: u16 = 1;
    let mut pixel_bars_placed: u8 = 0;
    let mut place = |kind: FixtureKind| {
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
            pos: None,
            profile_id: profile_id.to_string(),
            mode: String::new(),
            universe,
            start_address,
            unpatched: false,
        });
    };

    use FixtureKind::*;
    for kind in [Spot, Spot, Wash, Wash, Wash, Wash, Spot, Spot]
        .into_iter()
        .chain([Par; 6])
        .chain([Strobe; 2])
        .chain([PixelBar; 8])
        .chain([Smoke; 2])
        .chain([Pyro; 2])
    {
        place(kind);
    }

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
        assert_eq!(rig.placed(), 0, "arranged automatically");
        assert_eq!(rig.ids_of_kind(FixtureKind::Spot).count(), 4);
        assert_eq!(rig.ids_of_kind(FixtureKind::Wash).count(), 4);
        assert_eq!(rig.ids_of_kind(FixtureKind::Par).count(), 6);
        assert_eq!(rig.ids_of_kind(FixtureKind::Strobe).count(), 2);
        assert_eq!(rig.ids_of_kind(FixtureKind::PixelBar).count(), 8);
        assert_eq!(rig.ids_of_kind(FixtureKind::Smoke).count(), 2);
        assert_eq!(rig.ids_of_kind(FixtureKind::Pyro).count(), 2);
        // Seven kinds sharing four rows.
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

    /// `n` fixtures of `kind`, ids from `first`, on universe 1.
    fn of_kind(kind: FixtureKind, first: u32, n: u32) -> Vec<Fixture> {
        (first..first + n)
            .map(|id| Fixture {
                kind,
                ..at(id, PAR, 1, 1)
            })
            .collect()
    }

    fn cells_unique(rig: &Rig) {
        let cells = layout(rig);
        assert_eq!(cells.len(), rig.iter().count(), "every fixture has a cell");
        let unique: HashSet<Cell> = cells.values().copied().collect();
        assert_eq!(unique.len(), cells.len(), "no two share a cell");
        assert!(
            unique
                .iter()
                .all(|c| c.col < STAGE_COLS && c.row < STAGE_ROWS)
        );
    }

    #[test]
    fn no_two_fixtures_share_a_cell() {
        cells_unique(&default_rig(&FixtureLibrary::new()));
        // A full stage: 64 PARs.
        cells_unique(&Rig::from_fixtures(of_kind(FixtureKind::Par, 1, 64)));
        // Hand-placed fixtures sitting where the auto rows would go.
        let mut fixtures = of_kind(FixtureKind::Spot, 1, 4);
        fixtures.extend(of_kind(FixtureKind::Wash, 10, 4));
        fixtures[4].pos = Some(Cell::new(2, 0));
        fixtures[5].pos = Some(Cell::new(3, 0));
        cells_unique(&Rig::from_fixtures(fixtures));
    }

    #[test]
    fn auto_rows_share_kinds_that_fit_and_are_centred() {
        let rig = default_rig(&FixtureLibrary::new());
        let cells = layout(&rig);
        let at = |label: &str| {
            let f = rig.iter().find(|f| f.label == label).unwrap();
            cells[&f.id]
        };
        let row = |labels: &[&str]| labels.iter().map(|l| at(l)).collect::<Vec<_>>();
        let cells_of =
            |row: u8, cols: &[u8]| cols.iter().map(|&c| Cell::new(c, row)).collect::<Vec<_>>();
        // 4 spots and 4 washes fill row 0, in patch order, no room for a gap.
        assert_eq!(
            row(&["S1", "S2", "S3", "S4", "W1", "W2", "W3", "W4"]),
            cells_of(0, &[0, 1, 2, 3, 4, 5, 6, 7])
        );
        // 6 PARs and 2 strobes fill row 1.
        assert_eq!(row(&["P1", "P6", "ST1", "ST2"]), cells_of(1, &[0, 5, 6, 7]));
        // 2 smoke and 2 pyro, a cell apart, centred: 5 wide from column 1.
        assert_eq!(
            row(&["SM1", "SM2", "PY1", "PY2"]),
            cells_of(2, &[1, 2, 4, 5])
        );
        // 8 pixel bars, too many to join them, on a row of their own.
        assert_eq!(row(&["PB1", "PB8"]), cells_of(3, &[0, 7]));
        // One kind alone is centred: 3 washes from column 2.
        let rig = Rig::from_fixtures(of_kind(FixtureKind::Wash, 1, 3));
        let cells = layout(&rig);
        assert_eq!((cells[&1], cells[&3]), (Cell::new(2, 0), Cell::new(4, 0)));
    }

    #[test]
    fn hand_placed_cells_are_kept() {
        let mut fixtures = of_kind(FixtureKind::Spot, 1, 4);
        fixtures[0].pos = Some(Cell::new(0, 7));
        // A wash placed where S4 would go: S4 moves on to the next free.
        fixtures.extend(of_kind(FixtureKind::Wash, 10, 1));
        fixtures[4].pos = Some(Cell::new(4, 0));
        let rig = Rig::from_fixtures(fixtures);
        let cells = layout(&rig);
        assert_eq!(cells[&1], Cell::new(0, 7));
        assert_eq!(cells[&10], Cell::new(4, 0));
        // The others keep their slots, centred on all five and a gap
        // (spots from column 1): S1's at column 1 stays empty.
        assert_eq!(cells[&2], Cell::new(2, 0));
        assert_eq!(cells[&3], Cell::new(3, 0));
        assert_eq!(cells[&4], Cell::new(5, 0));
        assert_eq!(rig.placed(), 2);
        // Off the stage, or a cell already taken: arranged automatically.
        let mut fixtures = of_kind(FixtureKind::Spot, 1, 2);
        fixtures[0].pos = Some(Cell::new(9, 0));
        fixtures[1].pos = Some(Cell::new(0, 0));
        fixtures.extend(of_kind(FixtureKind::Wash, 10, 1));
        fixtures[2].pos = Some(Cell::new(0, 0));
        let cells = layout(&Rig::from_fixtures(fixtures));
        assert_eq!(cells[&2], Cell::new(0, 0));
        assert_ne!(cells[&10], Cell::new(0, 0));
        assert_ne!(cells[&1], Cell::new(0, 0));
    }

    #[test]
    fn a_kind_of_more_than_eight_wraps() {
        // 10 PARs: row 0 full, then on into row 1.
        let mut fixtures = of_kind(FixtureKind::Par, 1, 10);
        fixtures.extend(of_kind(FixtureKind::Strobe, 20, 2));
        let rig = Rig::from_fixtures(fixtures);
        let cells = layout(&rig);
        assert_eq!(cells[&1], Cell::new(0, 0));
        assert_eq!(cells[&8], Cell::new(7, 0));
        assert_eq!((cells[&9], cells[&10]), (Cell::new(0, 1), Cell::new(1, 1)));
        // The strobes start the next row, centred.
        assert_eq!((cells[&20], cells[&21]), (Cell::new(3, 2), Cell::new(4, 2)));
        cells_unique(&rig);
    }

    #[test]
    fn a_v1_file_loads_arranged_automatically() {
        let json = r#"{"version":1,"fixtures":[
            {"id":1,"kind":"Spot","label":"S1","col":3,"row":2,
             "profile_id":"shehds-led-spot-60w","universe":1,"start_address":12},
            {"id":2,"kind":"Par","label":"P1","col":0,"row":0,
             "profile_id":"shehds-rgbw-par","mode":"8CH","universe":2,
             "start_address":40,"unpatched":true}
        ]}"#;
        let rig = serde_json::from_str::<RigFile>(json).unwrap().into_rig();
        let s1 = rig.iter().find(|f| f.label == "S1").unwrap();
        assert_eq!((s1.universe, s1.start_address, s1.pos), (1, 12, None));
        let p1 = rig.iter().find(|f| f.label == "P1").unwrap();
        assert_eq!((p1.universe, p1.start_address), (2, 40));
        assert!(p1.unpatched);
        assert_eq!(rig.placed(), 0);
    }

    #[test]
    fn hand_placed_cells_roundtrip_and_auto_ones_stay_out() {
        let mut rig = default_rig(&FixtureLibrary::new());
        rig.fixtures_mut()[0].pos = Some(Cell::new(7, 7));
        let file = RigFile::from_rig(&rig);
        assert_eq!(file.version, 2);
        let json = serde_json::to_string(&file).unwrap();
        assert_eq!(json.matches("\"pos\"").count(), 1, "only the placed one");
        let back = serde_json::from_str::<RigFile>(&json).unwrap().into_rig();
        assert_eq!(back.iter().next().unwrap().pos, Some(Cell::new(7, 7)));
        assert_eq!(back.placed(), 1);
    }

    /// A fixture of `profile` at `universe`.`addr`, for the address tests.
    fn at(id: u32, profile: &str, universe: u8, start_address: u16) -> Fixture {
        Fixture {
            id,
            kind: FixtureKind::Par,
            label: format!("F{id}"),
            pos: None,
            profile_id: profile.into(),
            mode: String::new(),
            universe,
            start_address,
            unpatched: false,
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
    fn an_unpatched_fixture_takes_no_channels_and_keeps_its_address() {
        let library = FixtureLibrary::new();
        let mut rig = Rig::from_fixtures(vec![at(1, PAR, 1, 1), at(2, PAR, 1, 5)]);
        assert_eq!(rig.conflicts(&library).len(), 2);
        rig.fixtures_mut()[1].unpatched = true;
        assert!(rig.conflicts(&library).is_empty());
        assert_eq!(rig.occupancy(&library)[&1].used(), 8);
        assert_eq!(rig.first_free(&library, 1, 8, None), Some(9));
        // Alone on its universe, it doesn't add one.
        rig.fixtures_mut()[1].universe = 7;
        assert!(!rig.occupancy(&library).contains_key(&7));
        assert_eq!(rig.patched().map(|f| f.id).collect::<Vec<_>>(), [1]);
        // It remembers where it was, and old patches load patched.
        let f = rig.iter().nth(1).unwrap();
        assert_eq!((f.universe, f.start_address), (7, 5));
        let json = r#"{"id":1,"kind":"Par","label":"P1","col":0,"row":0,
            "profile_id":"shehds-rgbw-par","universe":1,"start_address":1}"#;
        let f: Fixture = serde_json::from_str(json).unwrap();
        assert!(!f.unpatched);
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
