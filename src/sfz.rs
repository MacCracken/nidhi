//! SFZ format parser — converts SFZ text files into nidhi [`Instrument`] + [`Zone`] structures.
//!
//! SFZ is a plain-text sampler instrument format with headers (`<region>`, `<group>`, `<global>`)
//! and opcode `key=value` pairs. This module provides:
//!
//! - [`SfzRegion`] — intermediate parse result for one region/group/global section
//! - [`SfzFile`] — the complete parsed file with global, group, and region data
//! - [`parse`] — parse SFZ text into an [`SfzFile`]
//! - [`SfzFile::to_instrument`] — convert to a nidhi [`Instrument`] + sample filename list
//! - [`SfzFile::to_zones`] — convert to `(Zone, sample_filename)` pairs

use alloc::string::String;
use alloc::vec::Vec;

use crate::envelope::AdsrConfig;
use crate::error::Result;
use crate::instrument::Instrument;
use crate::loop_mode::LoopMode;
use crate::sample::SampleId;
use crate::zone::{FilterMode, Zone};

/// Parse a note name (e.g., `c4`, `f#3`, `eb5`) or numeric MIDI value to a MIDI note number.
///
/// Supports C-1 through G9 (MIDI 0–127). Accidentals: `#` or `s` for sharp, `b` for flat.
#[must_use]
pub fn parse_note_or_number(s: &str) -> Option<u8> {
    // Try numeric first
    if let Ok(v) = s.parse::<u8>() {
        return Some(v);
    }

    let bytes = s.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    // Note base: c=0, d=2, e=4, f=5, g=7, a=9, b=11
    let note_base = match bytes[0].to_ascii_lowercase() {
        b'c' => 0i32,
        b'd' => 2,
        b'e' => 4,
        b'f' => 5,
        b'g' => 7,
        b'a' => 9,
        b'b' => 11,
        _ => return None,
    };

    let mut idx = 1;
    let mut accidental = 0i32;

    // Check for accidental
    if idx < bytes.len() {
        match bytes[idx] {
            b'#' | b's' => {
                accidental = 1;
                idx += 1;
            }
            b'b' if idx + 1 < bytes.len() && bytes[idx + 1].is_ascii_digit() => {
                // Only treat 'b' as flat if followed by digit (else it's note B)
                accidental = -1;
                idx += 1;
            }
            _ => {}
        }
    }

    // Parse octave (may be negative, e.g. "c-1")
    let octave_str = &s[idx..];
    let octave: i32 = octave_str.parse().ok()?;

    let midi = (octave + 1) * 12 + note_base + accidental;
    if (0..=127).contains(&midi) {
        Some(midi as u8)
    } else {
        None
    }
}

/// Intermediate representation of one SFZ section's opcodes.
///
/// Stores all parsed opcodes for a `<region>`, `<group>`, or `<global>` section.
/// Fields use `Option` or sentinel defaults so that inheritance (global → group → region)
/// can be resolved: a `None` or default value means "inherit from parent".
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[must_use]
pub struct SfzRegion {
    /// Sample filename (relative to SFZ file location).
    pub sample: Option<String>,
    /// Low key of the key range (default 0).
    pub lokey: u8,
    /// High key of the key range (default 127).
    pub hikey: u8,
    /// Low velocity of the velocity range (default 1).
    pub lovel: u8,
    /// High velocity of the velocity range (default 127).
    pub hivel: u8,
    /// Root note — the MIDI note at which the sample plays at original pitch (default 60).
    pub pitch_keycenter: u8,
    /// Fine tuning in cents.
    pub tune: i32,
    /// Volume in dB.
    pub volume: f32,
    /// Pan position (-100 to 100, mapped to -1.0..1.0 on export).
    pub pan: f32,
    /// Loop mode string: `"no_loop"`, `"loop_continuous"`, `"loop_sustain"`, `"one_shot"`.
    pub loop_mode: Option<String>,
    /// Loop start frame.
    pub loop_start: usize,
    /// Loop end frame.
    pub loop_end: usize,
    /// Round-robin group (SFZ `seq_position`).
    pub group: u32,
    /// Amplitude envelope attack time in seconds.
    pub ampeg_attack: f32,
    /// Amplitude envelope decay time in seconds.
    pub ampeg_decay: f32,
    /// Amplitude envelope sustain level (0–100, mapped to 0.0–1.0 on export).
    pub ampeg_sustain: f32,
    /// Amplitude envelope release time in seconds.
    pub ampeg_release: f32,
    /// Lowpass filter cutoff in Hz.
    pub cutoff: f32,
    /// Filter velocity tracking in cents (mapped to 0.0–1.0 on export).
    pub fil_veltrack: f32,
    /// Filter envelope attack time in seconds.
    pub fileg_attack: f32,
    /// Filter envelope decay time in seconds.
    pub fileg_decay: f32,
    /// Filter envelope sustain level (0–100).
    pub fileg_sustain: f32,
    /// Filter envelope release time in seconds.
    pub fileg_release: f32,
    /// Filter envelope depth in cents.
    pub fileg_depth: f32,
    /// Transpose in semitones.
    pub transpose: i32,
    /// Sample start offset in frames.
    pub offset: usize,
    /// Sample end position in frames (0 = full sample).
    pub end: usize,
    /// Filter resonance (Q factor).
    pub resonance: f32,
    /// Filter type string.
    pub fil_type: Option<String>,
    /// `key` shorthand (sets lokey=hikey=pitch_keycenter).
    pub key: Option<u8>,
}

impl Default for SfzRegion {
    fn default() -> Self {
        Self {
            sample: None,
            lokey: 0,
            hikey: 127,
            lovel: 1,
            hivel: 127,
            pitch_keycenter: 60,
            tune: 0,
            volume: 0.0,
            pan: 0.0,
            loop_mode: None,
            loop_start: 0,
            loop_end: 0,
            group: 0,
            ampeg_attack: 0.0,
            ampeg_decay: 0.0,
            ampeg_sustain: 100.0,
            ampeg_release: 0.0,
            cutoff: 0.0,
            fil_veltrack: 0.0,
            fileg_attack: 0.0,
            fileg_decay: 0.0,
            fileg_sustain: 100.0,
            fileg_release: 0.0,
            fileg_depth: 0.0,
            transpose: 0,
            offset: 0,
            end: 0,
            resonance: 0.0,
            fil_type: None,
            key: None,
        }
    }
}

impl SfzRegion {
    /// Create a new `SfzRegion` with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply an opcode key=value pair, ignoring unknown opcodes.
    fn apply_opcode(&mut self, key: &str, value: &str) {
        match key {
            "sample" => self.sample = Some(String::from(value)),
            "lokey" => {
                if let Some(v) = parse_note_or_number(value) {
                    self.lokey = v;
                }
            }
            "hikey" => {
                if let Some(v) = parse_note_or_number(value) {
                    self.hikey = v;
                }
            }
            "key" => {
                if let Some(v) = parse_note_or_number(value) {
                    self.key = Some(v);
                }
            }
            "lovel" => {
                if let Ok(v) = value.parse::<u8>() {
                    self.lovel = v;
                }
            }
            "hivel" => {
                if let Ok(v) = value.parse::<u8>() {
                    self.hivel = v;
                }
            }
            "pitch_keycenter" => {
                if let Some(v) = parse_note_or_number(value) {
                    self.pitch_keycenter = v;
                }
            }
            "tune" => {
                if let Ok(v) = value.parse::<i32>() {
                    self.tune = v;
                }
            }
            "volume" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.volume = v;
                }
            }
            "pan" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.pan = v.clamp(-100.0, 100.0);
                }
            }
            "loop_mode" | "loopmode" => self.loop_mode = Some(String::from(value)),
            "loop_start" | "loopstart" => {
                if let Ok(v) = value.parse::<usize>() {
                    self.loop_start = v;
                }
            }
            "loop_end" | "loopend" => {
                if let Ok(v) = value.parse::<usize>() {
                    self.loop_end = v;
                }
            }
            "seq_position" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.group = v;
                }
            }
            "group" => {
                if let Ok(v) = value.parse::<u32>() {
                    self.group = v;
                }
            }
            "ampeg_attack" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.ampeg_attack = v.max(0.0);
                }
            }
            "ampeg_decay" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.ampeg_decay = v.max(0.0);
                }
            }
            "ampeg_sustain" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.ampeg_sustain = v.clamp(0.0, 100.0);
                }
            }
            "ampeg_release" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.ampeg_release = v.max(0.0);
                }
            }
            "cutoff" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.cutoff = v.max(0.0);
                }
            }
            "fil_veltrack" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.fil_veltrack = v;
                }
            }
            "fileg_attack" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.fileg_attack = v.max(0.0);
                }
            }
            "fileg_decay" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.fileg_decay = v.max(0.0);
                }
            }
            "fileg_sustain" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.fileg_sustain = v.clamp(0.0, 100.0);
                }
            }
            "fileg_release" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.fileg_release = v.max(0.0);
                }
            }
            "fileg_depth" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.fileg_depth = v.clamp(-9600.0, 9600.0);
                }
            }
            "transpose" => {
                if let Ok(v) = value.parse::<i32>() {
                    self.transpose = v;
                }
            }
            "offset" => {
                if let Ok(v) = value.parse::<usize>() {
                    self.offset = v;
                }
            }
            "end" => {
                if let Ok(v) = value.parse::<usize>() {
                    self.end = v;
                }
            }
            "resonance" | "fil_resonance" => {
                if let Ok(v) = value.parse::<f32>() {
                    self.resonance = v.max(0.0);
                }
            }
            "fil_type" | "filtype" => {
                self.fil_type = Some(String::from(value));
            }
            // Unknown opcodes are silently ignored per SFZ spec convention.
            _ => {}
        }
    }

    /// Merge another region's non-default values onto `self` (used for inheritance).
    ///
    /// Values from `parent` are applied only where `self` still has the default value.
    /// This implements the SFZ inheritance chain: global → group → region.
    fn inherit_from(&mut self, parent: &SfzRegion) {
        if self.sample.is_none() {
            self.sample.clone_from(&parent.sample);
        }
        // For numeric fields, we inherit by checking if they are at their defaults.
        // This is a pragmatic approach — explicit zero values in a child will be kept.
        if self.lokey == 0 && parent.lokey != 0 {
            self.lokey = parent.lokey;
        }
        if self.hikey == 127 && parent.hikey != 127 {
            self.hikey = parent.hikey;
        }
        if self.lovel == 1 && parent.lovel != 1 {
            self.lovel = parent.lovel;
        }
        if self.hivel == 127 && parent.hivel != 127 {
            self.hivel = parent.hivel;
        }
        if self.pitch_keycenter == 60 && parent.pitch_keycenter != 60 {
            self.pitch_keycenter = parent.pitch_keycenter;
        }
        if self.tune == 0 && parent.tune != 0 {
            self.tune = parent.tune;
        }
        if self.volume == 0.0 && parent.volume != 0.0 {
            self.volume = parent.volume;
        }
        if self.pan == 0.0 && parent.pan != 0.0 {
            self.pan = parent.pan;
        }
        if self.loop_mode.is_none() {
            self.loop_mode.clone_from(&parent.loop_mode);
        }
        if self.loop_start == 0 && parent.loop_start != 0 {
            self.loop_start = parent.loop_start;
        }
        if self.loop_end == 0 && parent.loop_end != 0 {
            self.loop_end = parent.loop_end;
        }
        if self.group == 0 && parent.group != 0 {
            self.group = parent.group;
        }
        if self.ampeg_attack == 0.0 && parent.ampeg_attack != 0.0 {
            self.ampeg_attack = parent.ampeg_attack;
        }
        if self.ampeg_decay == 0.0 && parent.ampeg_decay != 0.0 {
            self.ampeg_decay = parent.ampeg_decay;
        }
        if self.ampeg_sustain == 100.0 && parent.ampeg_sustain != 100.0 {
            self.ampeg_sustain = parent.ampeg_sustain;
        }
        if self.ampeg_release == 0.0 && parent.ampeg_release != 0.0 {
            self.ampeg_release = parent.ampeg_release;
        }
        if self.cutoff == 0.0 && parent.cutoff != 0.0 {
            self.cutoff = parent.cutoff;
        }
        if self.fil_veltrack == 0.0 && parent.fil_veltrack != 0.0 {
            self.fil_veltrack = parent.fil_veltrack;
        }
        if self.fileg_attack == 0.0 && parent.fileg_attack != 0.0 {
            self.fileg_attack = parent.fileg_attack;
        }
        if self.fileg_decay == 0.0 && parent.fileg_decay != 0.0 {
            self.fileg_decay = parent.fileg_decay;
        }
        if self.fileg_sustain == 100.0 && parent.fileg_sustain != 100.0 {
            self.fileg_sustain = parent.fileg_sustain;
        }
        if self.fileg_release == 0.0 && parent.fileg_release != 0.0 {
            self.fileg_release = parent.fileg_release;
        }
        if self.fileg_depth == 0.0 && parent.fileg_depth != 0.0 {
            self.fileg_depth = parent.fileg_depth;
        }
        if self.transpose == 0 && parent.transpose != 0 {
            self.transpose = parent.transpose;
        }
        if self.offset == 0 && parent.offset != 0 {
            self.offset = parent.offset;
        }
        if self.end == 0 && parent.end != 0 {
            self.end = parent.end;
        }
        if self.resonance == 0.0 && parent.resonance != 0.0 {
            self.resonance = parent.resonance;
        }
        if self.fil_type.is_none() {
            self.fil_type.clone_from(&parent.fil_type);
        }
        if self.key.is_none() {
            self.key = parent.key;
        }
    }
}

/// The current header context during parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HeaderKind {
    None,
    Control,
    Global,
    Group,
    Region,
    Curve,
}

/// A fully parsed SFZ file.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[must_use]
pub struct SfzFile {
    /// Global defaults applied to all regions.
    pub global: SfzRegion,
    /// Group-level defaults (one per `<group>` encountered).
    pub groups: Vec<SfzRegion>,
    /// All parsed `<region>` sections (before inheritance merging).
    pub regions: Vec<SfzRegion>,
    /// Which group index each region belongs to (`None` if no group was active).
    group_indices: Vec<Option<usize>>,
    /// Default path prefix for sample filenames (from `<control> default_path`).
    pub default_path: Option<String>,
}

impl SfzFile {
    /// Convert the parsed SFZ into a nidhi [`Instrument`] and a list of sample filenames.
    ///
    /// Sample filenames are returned in the order they appear; each region's zone uses a
    /// [`SampleId`] corresponding to the index in the returned `Vec<String>`.
    ///
    /// `sample_rate` is needed to convert ADSR times (seconds) to samples.
    pub fn to_instrument(&self, name: &str, sample_rate: f32) -> (Instrument, Vec<String>) {
        let zones_and_files = self.to_zones(sample_rate);
        let mut inst = Instrument::new(name);
        let mut sample_files: Vec<String> = Vec::new();

        for (zone, filename) in zones_and_files {
            // Deduplicate: find existing or add new
            let idx = sample_files
                .iter()
                .position(|f| f == &filename)
                .unwrap_or_else(|| {
                    let i = sample_files.len();
                    sample_files.push(filename);
                    i
                });

            // Remap zone's sample_id to the deduplicated index
            let mut z = zone;
            z.sample_id = SampleId(idx as u32);
            inst.add_zone(z);
        }

        (inst, sample_files)
    }

    /// Convert the parsed SFZ into `(Zone, sample_filename)` pairs.
    ///
    /// Each region becomes a [`Zone`] with the merged (global → group → region) opcodes.
    /// The `sample_filename` is the raw `sample` opcode value.
    /// Regions without a `sample` opcode are skipped.
    ///
    /// `sample_rate` is needed to convert ADSR times (seconds) to samples.
    #[must_use]
    pub fn to_zones(&self, sample_rate: f32) -> Vec<(Zone, String)> {
        let mut result = Vec::with_capacity(self.regions.len());

        for (i, region) in self.regions.iter().enumerate() {
            // Merge inheritance: global → group → region
            let mut merged = region.clone();

            // Apply group defaults if this region belongs to a group
            if let Some(Some(group_idx)) = self.group_indices.get(i)
                && let Some(group) = self.groups.get(*group_idx)
            {
                merged.inherit_from(group);
            }

            // Apply global defaults
            merged.inherit_from(&self.global);

            // Apply `key` shorthand: sets lokey=hikey=pitch_keycenter
            if let Some(k) = merged.key {
                if merged.lokey == 0 && merged.hikey == 127 {
                    merged.lokey = k;
                    merged.hikey = k;
                }
                if merged.pitch_keycenter == 60 {
                    merged.pitch_keycenter = k;
                }
            }

            // Skip regions without a sample
            let mut filename = match merged.sample {
                Some(ref f) => f.clone(),
                None => continue,
            };

            // Prepend default_path if set
            if let Some(ref prefix) = self.default_path
                && !filename.starts_with(prefix.as_str())
            {
                let mut full = String::with_capacity(prefix.len() + filename.len());
                full.push_str(prefix);
                full.push_str(&filename);
                filename = full;
            }

            // Apply transpose to tune
            let tune_cents = merged.tune as f32 + merged.transpose as f32 * 100.0;

            // Map filter type
            let filter_type = map_fil_type(merged.fil_type.as_deref());

            // Placeholder sample ID — caller remaps after loading samples
            let mut zone = Zone::new(SampleId(i as u32))
                .with_key_range(merged.lokey, merged.hikey)
                .with_vel_range(merged.lovel, merged.hivel)
                .with_root_note(merged.pitch_keycenter)
                .with_tune(tune_cents)
                .with_volume(merged.volume)
                .with_pan(merged.pan / 100.0)
                .with_loop(
                    map_loop_mode(merged.loop_mode.as_deref()),
                    merged.loop_start,
                    merged.loop_end,
                )
                .with_filter(merged.cutoff, map_fil_veltrack(merged.fil_veltrack))
                .with_filter_type(filter_type)
                .with_group(merged.group);

            if merged.resonance > 0.0 {
                zone = zone.with_filter_resonance(merged.resonance);
            }
            if merged.offset > 0 {
                zone = zone.with_sample_offset(merged.offset);
            }
            if merged.end > 0 {
                zone = zone.with_sample_end(merged.end);
            }

            // Wire ADSR if any ampeg opcode was explicitly set
            let has_ampeg = merged.ampeg_attack != 0.0
                || merged.ampeg_decay != 0.0
                || merged.ampeg_sustain != 100.0
                || merged.ampeg_release != 0.0;

            let zone = if has_ampeg {
                let adsr = AdsrConfig::from_seconds(
                    merged.ampeg_attack,
                    merged.ampeg_decay,
                    merged.ampeg_sustain / 100.0,
                    merged.ampeg_release,
                    sample_rate,
                );
                zone.with_adsr(adsr)
            } else {
                zone
            };

            // Wire filter envelope if any fileg opcode was set
            let has_fileg = merged.fileg_depth != 0.0
                || merged.fileg_attack != 0.0
                || merged.fileg_decay != 0.0
                || merged.fileg_sustain != 100.0
                || merged.fileg_release != 0.0;

            let zone = if has_fileg {
                let fileg = AdsrConfig::from_seconds(
                    merged.fileg_attack,
                    merged.fileg_decay,
                    merged.fileg_sustain / 100.0,
                    merged.fileg_release,
                    sample_rate,
                );
                zone.with_filter_envelope(fileg, merged.fileg_depth)
            } else {
                zone
            };

            result.push((zone, filename));
        }

        result
    }
}

/// Map an SFZ `loop_mode` string to a nidhi [`LoopMode`].
#[must_use]
#[inline]
fn map_loop_mode(mode: Option<&str>) -> LoopMode {
    match mode {
        Some("loop_continuous") => LoopMode::Forward,
        Some("loop_sustain") => LoopMode::LoopSustain,
        Some("one_shot") => LoopMode::OneShot,
        Some("no_loop") | None => LoopMode::OneShot,
        Some(_) => LoopMode::OneShot,
    }
}

/// Map SFZ `fil_type` string to a nidhi [`FilterMode`].
#[must_use]
#[inline]
fn map_fil_type(fil_type: Option<&str>) -> FilterMode {
    match fil_type {
        Some("hpf_1p") | Some("hpf_2p") => FilterMode::HighPass,
        Some("bpf_2p") => FilterMode::BandPass,
        Some("brf_2p") => FilterMode::Notch,
        // lpf_1p, lpf_2p, or unknown → LowPass (default)
        _ => FilterMode::LowPass,
    }
}

/// Map SFZ `fil_veltrack` (in cents, typically 0–9600) to a 0.0–1.0 range.
#[must_use]
#[inline]
fn map_fil_veltrack(cents: f32) -> f32 {
    // SFZ fil_veltrack is in cents of filter cutoff change over velocity range.
    // 9600 cents = full range. We normalize to 0..1.
    (cents / 9600.0).clamp(0.0, 1.0)
}

/// Parse SFZ text into an [`SfzFile`].
///
/// The parser is line-based:
/// 1. Track the current header type (`<global>`, `<group>`, `<region>`)
/// 2. Split each line by whitespace into tokens
/// 3. Split each token by `=` into key/value opcode pairs
/// 4. Accumulate opcodes into the current section's [`SfzRegion`]
///
/// Unknown opcodes are silently ignored. Malformed lines (no `=`) are skipped.
/// Comments (lines starting with `//`) are stripped.
pub fn parse(input: &str) -> Result<SfzFile> {
    let mut global = SfzRegion::new();
    let mut groups: Vec<SfzRegion> = Vec::new();
    let mut regions: Vec<SfzRegion> = Vec::new();
    let mut group_indices: Vec<Option<usize>> = Vec::new();

    let mut current_header = HeaderKind::None;
    let mut current_group_idx: Option<usize> = None;
    let mut default_path: Option<String> = None;

    for line in input.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        // Tokenize the line by whitespace
        let tokens: Vec<&str> = line.split_whitespace().collect();

        for token in &tokens {
            // Check for headers
            if let Some(header) = parse_header(token) {
                match header {
                    HeaderKind::Control => {
                        current_header = HeaderKind::Control;
                    }
                    HeaderKind::Global => {
                        current_header = HeaderKind::Global;
                    }
                    HeaderKind::Group => {
                        current_header = HeaderKind::Group;
                        groups.push(SfzRegion::new());
                        current_group_idx = Some(groups.len() - 1);
                    }
                    HeaderKind::Region => {
                        current_header = HeaderKind::Region;
                        regions.push(SfzRegion::new());
                        group_indices.push(current_group_idx);
                    }
                    HeaderKind::Curve => {
                        current_header = HeaderKind::Curve;
                        // Curve opcodes are stored but not yet used
                    }
                    HeaderKind::None => {}
                }
                continue;
            }

            // Parse opcode key=value
            if let Some((key, value)) = split_opcode(token) {
                match current_header {
                    HeaderKind::Control => {
                        if key == "default_path" {
                            default_path = Some(String::from(value));
                        }
                        // Other control opcodes ignored for now
                    }
                    HeaderKind::Global => global.apply_opcode(key, value),
                    HeaderKind::Group => {
                        if let Some(g) = groups.last_mut() {
                            g.apply_opcode(key, value);
                        }
                    }
                    HeaderKind::Region => {
                        if let Some(r) = regions.last_mut() {
                            r.apply_opcode(key, value);
                        }
                    }
                    HeaderKind::Curve => {
                        // Curve opcodes stored for future use
                    }
                    HeaderKind::None => {
                        // Opcodes before any header are treated as global
                        global.apply_opcode(key, value);
                    }
                }
            }
        }
    }

    Ok(SfzFile {
        global,
        groups,
        regions,
        group_indices,
        default_path,
    })
}

/// Try to parse a token as a header (`<global>`, `<group>`, `<region>`).
fn parse_header(token: &str) -> Option<HeaderKind> {
    let trimmed = token.trim();
    if trimmed.starts_with('<') && trimmed.ends_with('>') {
        let name = &trimmed[1..trimmed.len() - 1];
        match name {
            "control" => Some(HeaderKind::Control),
            "global" => Some(HeaderKind::Global),
            "group" => Some(HeaderKind::Group),
            "region" => Some(HeaderKind::Region),
            "curve" => Some(HeaderKind::Curve),
            _ => None,
        }
    } else {
        None
    }
}

/// Split a token at the first `=` into (key, value).
fn split_opcode(token: &str) -> Option<(&str, &str)> {
    let idx = token.find('=')?;
    let key = &token[..idx];
    let value = &token[idx + 1..];
    if key.is_empty() || value.is_empty() {
        return None;
    }
    Some((key, value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_empty_file() {
        let sfz = parse("").expect("should parse empty input");
        assert!(sfz.regions.is_empty());
        assert!(sfz.groups.is_empty());
    }

    #[test]
    fn parse_single_region() {
        let input = r#"
<region>
sample=piano_c4.wav
lokey=60 hikey=72
pitch_keycenter=66
lovel=1 hivel=100
"#;
        let sfz = parse(input).expect("should parse single region");
        assert_eq!(sfz.regions.len(), 1);

        let r = &sfz.regions[0];
        assert_eq!(r.sample.as_deref(), Some("piano_c4.wav"));
        assert_eq!(r.lokey, 60);
        assert_eq!(r.hikey, 72);
        assert_eq!(r.pitch_keycenter, 66);
        assert_eq!(r.lovel, 1);
        assert_eq!(r.hivel, 100);
    }

    #[test]
    fn parse_with_global_defaults() {
        let input = r#"
<global>
ampeg_release=0.5
volume=-6

<region>
sample=test.wav
"#;
        let sfz = parse(input).expect("should parse with globals");
        assert_eq!(sfz.regions.len(), 1);
        assert!((sfz.global.ampeg_release - 0.5).abs() < f32::EPSILON);
        assert!((sfz.global.volume - -6.0).abs() < f32::EPSILON);

        // Convert and verify inheritance
        let zones = sfz.to_zones(44100.0);
        assert_eq!(zones.len(), 1);
        let (zone, filename) = &zones[0];
        assert_eq!(filename, "test.wav");
        assert!((zone.volume_db - -6.0).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_with_group_inheritance() {
        let input = r#"
<global>
ampeg_release=0.3

<group>
lokey=60 hikey=72

<region>
sample=soft.wav
lovel=1 hivel=80

<region>
sample=loud.wav
lovel=81 hivel=127
"#;
        let sfz = parse(input).expect("should parse with groups");
        assert_eq!(sfz.groups.len(), 1);
        assert_eq!(sfz.regions.len(), 2);

        // Group sets key range
        assert_eq!(sfz.groups[0].lokey, 60);
        assert_eq!(sfz.groups[0].hikey, 72);

        // Regions inherit group key range
        let zones = sfz.to_zones(44100.0);
        assert_eq!(zones.len(), 2);

        let (z0, f0) = &zones[0];
        assert_eq!(f0, "soft.wav");
        assert_eq!(z0.key_lo, 60);
        assert_eq!(z0.key_hi, 72);
        assert_eq!(z0.vel_lo, 1);
        assert_eq!(z0.vel_hi, 80);

        let (z1, f1) = &zones[1];
        assert_eq!(f1, "loud.wav");
        assert_eq!(z1.key_lo, 60);
        assert_eq!(z1.key_hi, 72);
        assert_eq!(z1.vel_lo, 81);
        assert_eq!(z1.vel_hi, 127);
    }

    #[test]
    fn round_trip_to_instrument() {
        let input = r#"
<region>
sample=piano.wav
lokey=48 hikey=72
pitch_keycenter=60
lovel=1 hivel=127
tune=5
volume=-3
pan=50
"#;
        let sfz = parse(input).expect("should parse");
        let (inst, files) = sfz.to_instrument("test_piano", 44100.0);

        assert_eq!(inst.name(), "test_piano");
        assert_eq!(inst.zone_count(), 1);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], "piano.wav");

        let zones = inst.zones();
        let z = &zones[0];
        assert_eq!(z.key_lo, 48);
        assert_eq!(z.key_hi, 72);
        assert_eq!(z.root_note, 60);
        assert!((z.tune_cents - 5.0).abs() < f32::EPSILON);
        assert!((z.volume_db - -3.0).abs() < f32::EPSILON);
        // pan=50 maps to 0.5
        assert!((z.pan - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn loop_mode_mapping() {
        assert_eq!(map_loop_mode(None), LoopMode::OneShot);
        assert_eq!(map_loop_mode(Some("no_loop")), LoopMode::OneShot);
        assert_eq!(map_loop_mode(Some("one_shot")), LoopMode::OneShot);
        assert_eq!(map_loop_mode(Some("loop_continuous")), LoopMode::Forward);
        assert_eq!(map_loop_mode(Some("loop_sustain")), LoopMode::LoopSustain);
        assert_eq!(map_loop_mode(Some("unknown_mode")), LoopMode::OneShot);
    }

    #[test]
    fn invalid_opcode_ignored() {
        let input = r#"
<region>
sample=test.wav
totally_fake_opcode=999
another_invalid=hello
lokey=60
"#;
        let sfz = parse(input).expect("should parse despite unknown opcodes");
        assert_eq!(sfz.regions.len(), 1);
        assert_eq!(sfz.regions[0].lokey, 60);
        assert_eq!(sfz.regions[0].sample.as_deref(), Some("test.wav"));
    }

    #[test]
    fn comments_and_blank_lines_skipped() {
        let input = r#"
// This is a comment
<region>
sample=test.wav

// Another comment
lokey=60 hikey=72
"#;
        let sfz = parse(input).expect("should parse");
        assert_eq!(sfz.regions.len(), 1);
        assert_eq!(sfz.regions[0].lokey, 60);
    }

    #[test]
    fn region_overrides_group_overrides_global() {
        let input = r#"
<global>
volume=-10
pan=25

<group>
volume=-5

<region>
sample=test.wav
volume=-2
"#;
        let sfz = parse(input).expect("should parse");
        let zones = sfz.to_zones(44100.0);
        assert_eq!(zones.len(), 1);

        let (z, _) = &zones[0];
        // Region sets volume=-2, should override group (-5) and global (-10)
        assert!((z.volume_db - -2.0).abs() < f32::EPSILON);
        // Pan inherited from global (25/100 = 0.25)
        assert!((z.pan - 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn loop_mode_parsed_in_region() {
        let input = r#"
<region>
sample=loop.wav
loop_mode=loop_continuous
loop_start=1000
loop_end=5000
"#;
        let sfz = parse(input).expect("should parse");
        let zones = sfz.to_zones(44100.0);
        assert_eq!(zones.len(), 1);

        let (z, _) = &zones[0];
        assert_eq!(z.loop_mode, LoopMode::Forward);
        assert_eq!(z.loop_start, 1000);
        assert_eq!(z.loop_end, 5000);
    }

    #[test]
    fn filter_opcodes_parsed() {
        let input = r#"
<region>
sample=test.wav
cutoff=5000
fil_veltrack=4800
"#;
        let sfz = parse(input).expect("should parse");
        let zones = sfz.to_zones(44100.0);
        assert_eq!(zones.len(), 1);

        let (z, _) = &zones[0];
        assert!((z.filter_cutoff - 5000.0).abs() < f32::EPSILON);
        // 4800/9600 = 0.5
        assert!((z.filter_vel_track - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn multiple_groups() {
        let input = r#"
<group>
lokey=36 hikey=47

<region>
sample=bass.wav

<group>
lokey=48 hikey=72

<region>
sample=mid.wav

<region>
sample=mid2.wav
"#;
        let sfz = parse(input).expect("should parse");
        assert_eq!(sfz.groups.len(), 2);
        assert_eq!(sfz.regions.len(), 3);

        let zones = sfz.to_zones(44100.0);
        assert_eq!(zones.len(), 3);

        // First region inherits from first group
        assert_eq!(zones[0].0.key_lo, 36);
        assert_eq!(zones[0].0.key_hi, 47);
        assert_eq!(zones[0].1, "bass.wav");

        // Second and third regions inherit from second group
        assert_eq!(zones[1].0.key_lo, 48);
        assert_eq!(zones[1].0.key_hi, 72);
        assert_eq!(zones[2].0.key_lo, 48);
        assert_eq!(zones[2].0.key_hi, 72);
    }

    #[test]
    fn region_without_sample_skipped() {
        let input = r#"
<region>
lokey=60 hikey=72

<region>
sample=valid.wav
"#;
        let sfz = parse(input).expect("should parse");
        assert_eq!(sfz.regions.len(), 2);

        // Only one zone produced (the one with a sample)
        let zones = sfz.to_zones(44100.0);
        assert_eq!(zones.len(), 1);
        assert_eq!(zones[0].1, "valid.wav");
    }

    #[test]
    fn adsr_envelope_from_sfz() {
        let input = r#"
<global>
ampeg_attack=0.01
ampeg_decay=0.1
ampeg_sustain=70
ampeg_release=0.5

<region>
sample=test.wav
"#;
        let sfz = parse(input).expect("should parse");
        assert!((sfz.global.ampeg_attack - 0.01).abs() < f32::EPSILON);
        assert!((sfz.global.ampeg_decay - 0.1).abs() < f32::EPSILON);
        assert!((sfz.global.ampeg_sustain - 70.0).abs() < f32::EPSILON);
        assert!((sfz.global.ampeg_release - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn to_instrument_deduplicates_samples() {
        let input = r#"
<region>
sample=shared.wav
lokey=60 hikey=66

<region>
sample=shared.wav
lokey=67 hikey=72

<region>
sample=other.wav
lokey=73 hikey=84
"#;
        let sfz = parse(input).expect("should parse");
        let (inst, files) = sfz.to_instrument("dedup_test", 44100.0);

        assert_eq!(inst.zone_count(), 3);
        // Only 2 unique sample files
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], "shared.wav");
        assert_eq!(files[1], "other.wav");

        // Both zones referencing shared.wav should have the same SampleId
        let zones = inst.zones();
        assert_eq!(zones[0].sample_id(), SampleId(0));
        assert_eq!(zones[1].sample_id(), SampleId(0));
        assert_eq!(zones[2].sample_id(), SampleId(1));
    }
}
