//! Per-instrument effect chain — routes audio through naad effects.
//!
//! When the `std` feature is enabled, provides a chain of up to 5 effect slots
//! using naad's effects (reverb, delay, chorus, compressor, etc.). Under `no_std`,
//! the chain is a no-op passthrough.

use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

/// Maximum number of effect slots per chain.
pub const MAX_SLOTS: usize = 5;

/// An effect slot — wraps a naad effect with bypass and wet/dry mix.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectSlot {
    /// Effect type identifier.
    pub effect_type: EffectType,
    /// Whether this slot is bypassed.
    pub bypass: bool,
    /// Wet/dry mix (0.0 = fully dry, 1.0 = fully wet).
    pub mix: f32,
    /// Slot-specific state (opaque, managed by the chain).
    #[cfg(feature = "std")]
    #[serde(skip)]
    state: EffectState,
}

/// Effect type selection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum EffectType {
    /// No effect (passthrough).
    #[default]
    None,
    /// Algorithmic reverb.
    Reverb,
    /// Delay line.
    Delay,
    /// Chorus.
    Chorus,
    /// Compressor.
    Compressor,
    /// Limiter.
    Limiter,
}

/// Internal effect state (std only).
#[cfg(feature = "std")]
#[derive(Debug, Clone, Default)]
enum EffectState {
    #[default]
    None,
    Reverb(alloc::boxed::Box<naad::reverb::Reverb>),
    Delay(naad::delay::CombFilter),
    Chorus(naad::effects::Chorus),
    Compressor(naad::dynamics::Compressor),
    Limiter(naad::dynamics::Limiter),
}

impl EffectSlot {
    /// Create an empty (passthrough) slot.
    #[must_use]
    pub fn new() -> Self {
        Self {
            effect_type: EffectType::None,
            bypass: false,
            mix: 1.0,
            #[cfg(feature = "std")]
            state: EffectState::None,
        }
    }
}

impl Default for EffectSlot {
    fn default() -> Self {
        Self::new()
    }
}

/// Per-instrument effect chain with up to 5 serial slots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EffectChain {
    slots: Vec<EffectSlot>,
    sample_rate: f32,
}

impl EffectChain {
    /// Create a new empty effect chain.
    #[must_use]
    pub fn new(sample_rate: f32) -> Self {
        Self {
            slots: Vec::new(),
            sample_rate,
        }
    }

    /// Add an effect to the chain. Returns false if chain is full.
    pub fn add(&mut self, effect_type: EffectType) -> bool {
        if self.slots.len() >= MAX_SLOTS {
            return false;
        }

        let mut slot = EffectSlot::new();
        slot.effect_type = effect_type;

        #[cfg(feature = "std")]
        {
            slot.state = self.create_state(effect_type);
        }

        self.slots.push(slot);
        true
    }

    /// Remove an effect by index.
    pub fn remove(&mut self, index: usize) {
        if index < self.slots.len() {
            self.slots.remove(index);
        }
    }

    /// Clear all effects.
    pub fn clear(&mut self) {
        self.slots.clear();
    }

    /// Get a reference to the slots.
    #[must_use]
    pub fn slots(&self) -> &[EffectSlot] {
        &self.slots
    }

    /// Get a mutable reference to a slot by index.
    pub fn slot_mut(&mut self, index: usize) -> Option<&mut EffectSlot> {
        self.slots.get_mut(index)
    }

    /// Number of active slots.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Whether the chain is empty.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Process a mono sample through the chain.
    #[inline]
    pub fn process_sample(&mut self, input: f32) -> f32 {
        #[allow(unused_mut)]
        let mut out = input;

        #[cfg(feature = "std")]
        for slot in &mut self.slots {
            if slot.bypass || matches!(slot.effect_type, EffectType::None) {
                continue;
            }
            let wet = match &mut slot.state {
                EffectState::None => out,
                EffectState::Reverb(r) => {
                    let (l, _r) = r.process_sample(out);
                    l
                }
                EffectState::Delay(d) => d.process_sample(out),
                EffectState::Chorus(c) => c.process_sample(out),
                EffectState::Compressor(c) => c.process_sample(out),
                EffectState::Limiter(l) => l.process_sample(out),
            };
            out = out * (1.0 - slot.mix) + wet * slot.mix;
        }

        #[cfg(not(feature = "std"))]
        {
            let _ = &self.slots; // no-op under no_std
        }

        out
    }

    /// Process a stereo sample pair through the chain.
    #[inline]
    pub fn process_stereo(&mut self, left: f32, right: f32) -> (f32, f32) {
        // Process each channel independently through the chain
        let l = self.process_sample(left);
        let r = self.process_sample(right);
        (l, r)
    }

    #[cfg(feature = "std")]
    fn create_state(&self, effect_type: EffectType) -> EffectState {
        let sr = self.sample_rate;
        match effect_type {
            EffectType::None => EffectState::None,
            EffectType::Reverb => naad::reverb::Reverb::new(sr, 1.5, 0.3, 0.5, 0.6)
                .map(|r| EffectState::Reverb(alloc::boxed::Box::new(r)))
                .unwrap_or(EffectState::None),
            EffectType::Delay => {
                let samples = (sr * 0.3) as usize; // 300ms delay
                EffectState::Delay(naad::delay::CombFilter::new(samples, 0.4))
            }
            EffectType::Chorus => naad::effects::Chorus::new(3, 0.02, 0.002, 1.5, 0.5, sr)
                .map(EffectState::Chorus)
                .unwrap_or(EffectState::None),
            EffectType::Compressor => {
                EffectState::Compressor(naad::dynamics::Compressor::new(-20.0, 4.0, 0.01, 0.1, sr))
            }
            EffectType::Limiter => {
                EffectState::Limiter(naad::dynamics::Limiter::new(-1.0, 0.05, sr))
            }
        }
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use super::*;

    #[test]
    fn empty_chain_passthrough() {
        let mut chain = EffectChain::new(44100.0);
        assert_eq!(chain.process_sample(0.5), 0.5);
    }

    #[test]
    fn max_slots_enforced() {
        let mut chain = EffectChain::new(44100.0);
        for _ in 0..MAX_SLOTS {
            assert!(chain.add(EffectType::None));
        }
        assert!(!chain.add(EffectType::None));
    }

    #[test]
    fn bypass_skips_effect() {
        let mut chain = EffectChain::new(44100.0);
        chain.add(EffectType::Compressor);
        chain.slot_mut(0).unwrap().bypass = true;
        // Should pass through unchanged
        let out = chain.process_sample(0.5);
        assert!((out - 0.5).abs() < 0.01);
    }

    #[test]
    fn remove_slot() {
        let mut chain = EffectChain::new(44100.0);
        chain.add(EffectType::Reverb);
        chain.add(EffectType::Delay);
        assert_eq!(chain.len(), 2);
        chain.remove(0);
        assert_eq!(chain.len(), 1);
    }

    #[test]
    fn wet_dry_mix() {
        let mut chain = EffectChain::new(44100.0);
        chain.add(EffectType::None); // passthrough
        chain.slot_mut(0).unwrap().mix = 0.0; // fully dry
        let out = chain.process_sample(0.5);
        assert!((out - 0.5).abs() < 0.01);
    }
}
