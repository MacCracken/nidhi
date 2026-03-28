//! Instrument — a collection of zones forming a playable sampled instrument.

use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::zone::Zone;

/// A sampled instrument — a collection of key/velocity zones.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[must_use]
pub struct Instrument {
    /// Instrument name.
    name: String,
    /// Zones ordered by key range for efficient lookup.
    zones: Vec<Zone>,
    /// Round-robin counters per group.
    rr_counters: Vec<u32>,
}

impl Instrument {
    /// Create a new empty instrument.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            zones: Vec::new(),
            rr_counters: Vec::new(),
        }
    }

    /// Add a zone to the instrument.
    pub fn add_zone(&mut self, zone: Zone) {
        let group = zone.group();
        self.zones.push(zone);
        // Ensure rr_counters covers this group
        if group > 0 {
            let needed = group as usize + 1;
            if self.rr_counters.len() < needed {
                self.rr_counters.resize(needed, 0);
            }
        }
    }

    /// Find all zones matching a MIDI note and velocity.
    #[must_use]
    pub fn find_zones(&self, note: u8, velocity: u8) -> Vec<&Zone> {
        self.zones.iter().filter(|z| z.matches(note, velocity)).collect()
    }

    /// Find a single zone using round-robin selection.
    ///
    /// For zones with `group > 0`, cycles through matching zones in that group.
    /// For zones with `group == 0` (ungrouped), returns the first match.
    /// Returns the zone index and a reference to the zone.
    pub fn find_zone_rr(&mut self, note: u8, velocity: u8) -> Option<(usize, &Zone)> {
        // Collect matching zone indices
        let matching: Vec<usize> = self
            .zones
            .iter()
            .enumerate()
            .filter(|(_, z)| z.matches(note, velocity))
            .map(|(i, _)| i)
            .collect();

        if matching.is_empty() {
            return None;
        }

        // Group 0 zones: return the first match
        // For grouped zones: use round-robin within the group
        let first_match = matching[0];
        let group = self.zones[first_match].group();

        if group == 0 {
            return Some((first_match, &self.zones[first_match]));
        }

        // Filter to only zones in this group
        let group_matches: Vec<usize> = matching
            .into_iter()
            .filter(|&i| self.zones[i].group() == group)
            .collect();

        if group_matches.is_empty() {
            return None;
        }

        // Ensure counter exists
        let needed = group as usize + 1;
        if self.rr_counters.len() < needed {
            self.rr_counters.resize(needed, 0);
        }

        let counter = &mut self.rr_counters[group as usize];
        let pick = (*counter as usize) % group_matches.len();
        *counter = counter.wrapping_add(1);

        let zone_idx = group_matches[pick];
        Some((zone_idx, &self.zones[zone_idx]))
    }

    /// Instrument name.
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Number of zones.
    #[inline]
    pub fn zone_count(&self) -> usize {
        self.zones.len()
    }

    /// Get all zones.
    pub fn zones(&self) -> &[Zone] {
        &self.zones
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::SampleId;

    #[test]
    fn instrument_find_zones() {
        let mut inst = Instrument::new("test");
        inst.add_zone(Zone::new(SampleId(0)).with_key_range(60, 72));
        inst.add_zone(Zone::new(SampleId(1)).with_key_range(48, 59));

        let zones = inst.find_zones(66, 100);
        assert_eq!(zones.len(), 1);
        assert_eq!(zones[0].sample_id(), SampleId(0));

        let zones = inst.find_zones(50, 100);
        assert_eq!(zones.len(), 1);
        assert_eq!(zones[0].sample_id(), SampleId(1));
    }

    #[test]
    fn round_robin_cycles() {
        let mut inst = Instrument::new("rr_test");
        inst.add_zone(
            Zone::new(SampleId(0))
                .with_key_range(60, 72)
                .with_group(1),
        );
        inst.add_zone(
            Zone::new(SampleId(1))
                .with_key_range(60, 72)
                .with_group(1),
        );
        inst.add_zone(
            Zone::new(SampleId(2))
                .with_key_range(60, 72)
                .with_group(1),
        );

        // Should cycle through 0, 1, 2, 0, 1, 2...
        let (idx0, z0) = inst.find_zone_rr(66, 100).unwrap();
        assert_eq!(z0.sample_id(), SampleId(0));
        assert_eq!(idx0, 0);

        let (idx1, z1) = inst.find_zone_rr(66, 100).unwrap();
        assert_eq!(z1.sample_id(), SampleId(1));
        assert_eq!(idx1, 1);

        let (idx2, z2) = inst.find_zone_rr(66, 100).unwrap();
        assert_eq!(z2.sample_id(), SampleId(2));
        assert_eq!(idx2, 2);

        // Wraps around
        let (idx3, z3) = inst.find_zone_rr(66, 100).unwrap();
        assert_eq!(z3.sample_id(), SampleId(0));
        assert_eq!(idx3, 0);
    }

    #[test]
    fn round_robin_ungrouped_returns_first() {
        let mut inst = Instrument::new("ungrouped");
        inst.add_zone(Zone::new(SampleId(0)).with_key_range(60, 72));
        inst.add_zone(Zone::new(SampleId(1)).with_key_range(60, 72));

        // group=0, so always returns first match
        let (idx0, _) = inst.find_zone_rr(66, 100).unwrap();
        let (idx1, _) = inst.find_zone_rr(66, 100).unwrap();
        assert_eq!(idx0, idx1);
    }

    #[test]
    fn round_robin_no_match() {
        let mut inst = Instrument::new("empty");
        inst.add_zone(Zone::new(SampleId(0)).with_key_range(60, 72).with_group(1));

        assert!(inst.find_zone_rr(50, 100).is_none());
    }
}
