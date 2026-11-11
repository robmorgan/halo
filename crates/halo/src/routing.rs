//! Output routing: which of a device's stereo pairs carries what. Pure
//! data, remembered per device and resolved into a [`Layout`] when the
//! stream opens.

/// A stereo pair of outputs: 0 is 1–2, 1 is 3–4, and so on.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct Pair(pub u8);

impl Pair {
    /// "3–4".
    pub fn label(self) -> String {
        let first = u16::from(self.0) * 2 + 1;
        format!("{first}–{}", first + 1)
    }

    /// The channels a stream needs to reach it.
    pub fn channels(self) -> u16 {
        (u16::from(self.0) + 1) * 2
    }

    /// Its left channel in an interleaved frame.
    pub fn offset(self) -> usize {
        usize::from(self.0) * 2
    }
}

/// One pair of a device, as Options shows it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PairConfig {
    /// A disabled pair carries nothing and can't be chosen.
    pub enabled: bool,
    /// What it's plugged into, e.g. "DJM CH 2"; empty for none.
    #[serde(default)]
    pub name: String,
}

/// The longest pair name.
pub const NAME_MAX: usize = 24;

/// One device's routing. Each role is a pair, or `None` for nowhere.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeviceRouting {
    pub device: String,
    /// A hardware mixer mixes: each deck goes to its own pair.
    #[serde(default)]
    pub external: bool,
    pub pairs: Vec<PairConfig>,
    /// Internal: the master, never nowhere.
    pub master: Pair,
    /// Internal: the headphone cue.
    #[serde(default)]
    pub phones: Option<Pair>,
    /// External: each deck, after trim only.
    #[serde(default)]
    pub deck_a: Option<Pair>,
    #[serde(default)]
    pub deck_b: Option<Pair>,
    /// External: the audition player, after its own volume.
    #[serde(default)]
    pub audition: Option<Pair>,
}

impl DeviceRouting {
    /// A device's routing before it's been edited: every pair on, the
    /// master on 1–2 and the phones on 3–4 (unless the cue was turned off
    /// before routing existed); for a hardware mixer A on 1–2, B and the
    /// audition player on 3–4.
    pub fn default_for(device: &str, channels: u16, cue_off: bool) -> Self {
        let count = (channels / 2).max(1);
        let second = (count >= 2).then_some(Pair(1));
        Self {
            device: device.to_string(),
            external: false,
            pairs: vec![
                PairConfig {
                    enabled: true,
                    name: String::new(),
                };
                usize::from(count)
            ],
            master: Pair(0),
            phones: second.filter(|_| !cue_off),
            deck_a: Some(Pair(0)),
            deck_b: second,
            audition: second,
        }
    }

    /// The saved routing for `device`, or its default.
    pub fn for_device(
        routings: &[DeviceRouting],
        device: &str,
        channels: u16,
        cue_off: bool,
    ) -> Self {
        routings
            .iter()
            .find(|r| r.device == device)
            .cloned()
            .unwrap_or_else(|| Self::default_for(device, channels, cue_off))
    }

    pub fn all_pairs(&self) -> impl Iterator<Item = Pair> {
        (0..self.pairs.len()).map(|i| Pair(i as u8))
    }

    pub fn enabled(&self, pair: Pair) -> bool {
        self.pairs
            .get(usize::from(pair.0))
            .is_some_and(|p| p.enabled)
    }

    pub fn enabled_pairs(&self) -> impl Iterator<Item = Pair> + '_ {
        self.all_pairs().filter(|&p| self.enabled(p))
    }

    /// The last enabled pair stays on.
    pub fn can_disable(&self, pair: Pair) -> bool {
        !self.enabled(pair) || self.enabled_pairs().count() > 1
    }

    /// Turn a pair on or off. Off, nothing plays on it: its roles go
    /// nowhere, and the master moves to the first pair still on.
    pub fn set_enabled(&mut self, pair: Pair, on: bool) {
        if !on && !self.can_disable(pair) {
            return;
        }
        let Some(config) = self.pairs.get_mut(usize::from(pair.0)) else {
            return;
        };
        config.enabled = on;
        if on {
            return;
        }
        for role in [
            &mut self.phones,
            &mut self.deck_a,
            &mut self.deck_b,
            &mut self.audition,
        ] {
            if *role == Some(pair) {
                *role = None;
            }
        }
        let first = self.enabled_pairs().next();
        if self.master == pair
            && let Some(first) = first
        {
            self.set_master(first);
        }
    }

    /// Move the master; the phones can't share its pair.
    pub fn set_master(&mut self, pair: Pair) {
        self.master = pair;
        if self.phones == Some(pair) {
            self.phones = None;
        }
    }

    /// A hardware mixer needs a pair per deck.
    pub fn can_external(&self) -> bool {
        self.pairs.len() >= 2
    }

    /// "1–2 · DJM CH 2", or "OUTPUTS 1–2" for a pair with no name.
    pub fn pair_label(&self, pair: Pair) -> String {
        match self.pairs.get(usize::from(pair.0)) {
            Some(PairConfig { name, .. }) if !name.trim().is_empty() => {
                format!("{} · {}", pair.label(), name.trim())
            }
            _ => format!("OUTPUTS {}", pair.label()),
        }
    }

    /// A pair's name, or "3–4" for one with no name.
    pub fn short_label(&self, pair: Pair) -> String {
        match self.pairs.get(usize::from(pair.0)) {
            Some(PairConfig { name, .. }) if !name.trim().is_empty() => name.trim().to_string(),
            _ => pair.label(),
        }
    }

    /// What a stream that can open up to `max_channels` plays: roles on
    /// disabled pairs, or past the device, go nowhere.
    pub fn resolve(&self, max_channels: u16) -> Layout {
        let usable = |pair: Option<Pair>| {
            pair.filter(|&p| self.enabled(p) && p.channels() <= max_channels.max(2))
        };
        let mixing = if self.external && self.can_external() {
            Mixing::External {
                deck_a: usable(self.deck_a),
                deck_b: usable(self.deck_b),
                audition: usable(self.audition),
            }
        } else {
            let master = usable(Some(self.master)).unwrap_or(Pair(0));
            Mixing::Internal {
                master,
                phones: usable(self.phones).filter(|&p| p != master),
            }
        };
        Layout {
            channels: mixing.pairs().map(Pair::channels).max().unwrap_or(2),
            mixing,
        }
    }
}

/// What a stream's outputs carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mixing {
    /// Halo mixes: the master, and the phones' cue on their own pair.
    Internal { master: Pair, phones: Option<Pair> },
    /// A hardware mixer mixes: each deck and the audition player on the
    /// pair chosen for it.
    External {
        deck_a: Option<Pair>,
        deck_b: Option<Pair>,
        audition: Option<Pair>,
    },
}

impl Mixing {
    /// The pairs something plays on.
    pub fn pairs(&self) -> impl Iterator<Item = Pair> {
        let roles = match *self {
            Mixing::Internal { master, phones } => [Some(master), phones, None],
            Mixing::External {
                deck_a,
                deck_b,
                audition,
            } => [deck_a, deck_b, audition],
        };
        roles.into_iter().flatten()
    }
}

/// A stream's channel count and what its pairs carry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub channels: u16,
    pub mixing: Mixing,
}

impl Layout {
    /// The master alone on a stereo stream.
    pub fn stereo() -> Self {
        Self {
            channels: 2,
            mixing: Mixing::Internal {
                master: Pair(0),
                phones: None,
            },
        }
    }

    /// The phones' pair, when Halo mixes and has one.
    pub fn phones(&self) -> Option<Pair> {
        match self.mixing {
            Mixing::Internal { phones, .. } => phones,
            Mixing::External { .. } => None,
        }
    }

    pub fn external(&self) -> bool {
        matches!(self.mixing, Mixing::External { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_follow_the_device() {
        let speakers = DeviceRouting::default_for("Speakers", 2, false);
        assert_eq!(speakers.pairs.len(), 1);
        assert_eq!((speakers.phones, speakers.deck_b), (None, None));
        assert!(!speakers.can_external());

        let z1 = DeviceRouting::default_for("Z1", 4, false);
        assert_eq!((z1.master, z1.phones), (Pair(0), Some(Pair(1))));
        assert_eq!(z1.resolve(4).channels, 4);
        // A cue turned off before routing existed stays off.
        assert_eq!(DeviceRouting::default_for("Z1", 4, true).phones, None);

        let big = DeviceRouting::default_for("Interface", 6, false);
        assert_eq!(big.pairs.len(), 3);
        assert!(big.pairs.iter().all(|p| p.enabled));
    }

    #[test]
    fn disabling_a_pair_clears_its_roles_and_keeps_one_on() {
        let mut r = DeviceRouting::default_for("M4", 4, false);
        r.set_enabled(Pair(1), false);
        assert_eq!((r.phones, r.deck_b, r.audition), (None, None, None));
        r.set_enabled(Pair(1), true);
        r.phones = Some(Pair(1));
        // Disabling the master's pair moves it, off the phones' pair.
        r.set_enabled(Pair(0), false);
        assert_eq!((r.master, r.phones), (Pair(1), None));
        // The last pair on stays on.
        assert!(!r.can_disable(Pair(1)));
        r.set_enabled(Pair(1), false);
        assert!(r.enabled(Pair(1)));
    }

    #[test]
    fn resolve_drops_what_can_not_play_and_opens_what_is_used() {
        let mut r = DeviceRouting::default_for("Interface", 6, false);
        r.external = true;
        r.deck_a = Some(Pair(2));
        r.audition = None;
        let layout = r.resolve(6);
        assert_eq!(layout.channels, 6);
        assert_eq!(
            layout.mixing,
            Mixing::External {
                deck_a: Some(Pair(2)),
                deck_b: Some(Pair(1)),
                audition: None,
            }
        );
        // A device that now opens only 4: deck A's pair is past it.
        let layout = r.resolve(4);
        assert_eq!(layout.channels, 4);
        assert!(matches!(
            layout.mixing,
            Mixing::External { deck_a: None, .. }
        ));

        // Internal with the phones on 1–2 and the master on 3–4.
        let mut r = DeviceRouting::default_for("M4", 4, false);
        r.set_master(Pair(1));
        r.phones = Some(Pair(0));
        assert_eq!(
            r.resolve(4).mixing,
            Mixing::Internal {
                master: Pair(1),
                phones: Some(Pair(0)),
            }
        );
        // On a stereo device only the master survives, on 1–2.
        assert_eq!(r.resolve(2), Layout::stereo());
    }

    #[test]
    fn labels_use_the_pair_names() {
        let mut r = DeviceRouting::default_for("M4", 4, false);
        r.pairs[0].name = " DJM CH 2 ".to_string();
        assert_eq!(r.pair_label(Pair(0)), "1–2 · DJM CH 2");
        assert_eq!(r.pair_label(Pair(1)), "OUTPUTS 3–4");
        assert_eq!(r.short_label(Pair(0)), "DJM CH 2");
        assert_eq!(r.short_label(Pair(1)), "3–4");
    }
}
