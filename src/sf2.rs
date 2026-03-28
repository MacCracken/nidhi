//! SoundFont 2 (SF2) binary file parser.
//!
//! Parses the RIFF-based SF2 format and extracts presets with their instruments
//! and sample data. The parser takes raw `&[u8]` bytes (no file I/O) and returns
//! nidhi-native types: [`Instrument`], [`SampleBank`], and [`Sf2Preset`] metadata.
//!
//! # Example
//!
//! ```rust,no_run
//! use nidhi::sf2;
//!
//! let bytes: &[u8] = &[]; // caller loads SF2 file bytes
//! let (presets, instruments, bank) = sf2::parse(bytes).unwrap();
//! ```

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use serde::{Deserialize, Serialize};

use crate::error::{NidhiError, Result};
use crate::instrument::Instrument;
use crate::loop_mode::LoopMode;
use crate::sample::{Sample, SampleBank};
use crate::zone::Zone;

// ── Public types ────────────────────────────────────────────────────────

/// Metadata for a parsed SF2 preset.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Sf2Preset {
    /// Preset name from the SF2 file.
    pub name: String,
    /// MIDI bank number.
    pub bank: u16,
    /// MIDI preset/program number.
    pub preset_number: u16,
}

// ── RIFF / SF2 constants ────────────────────────────────────────────────

const RIFF_ID: [u8; 4] = *b"RIFF";
const SFBK_ID: [u8; 4] = *b"sfbk";
const LIST_ID: [u8; 4] = *b"LIST";

const PHDR_ID: [u8; 4] = *b"phdr";
const PBAG_ID: [u8; 4] = *b"pbag";
const PGEN_ID: [u8; 4] = *b"pgen";
const INST_ID: [u8; 4] = *b"inst";
const IBAG_ID: [u8; 4] = *b"ibag";
const IGEN_ID: [u8; 4] = *b"igen";
const SHDR_ID: [u8; 4] = *b"shdr";

const GEN_INSTRUMENT: u16 = 41;
const GEN_KEY_RANGE: u16 = 43;
const GEN_VEL_RANGE: u16 = 44;
const GEN_SAMPLE_ID: u16 = 53;
const GEN_SAMPLE_MODES: u16 = 54;
const GEN_OVERRIDING_ROOT_KEY: u16 = 58;

// ── Raw SF2 record structs ──────────────────────────────────────────────

#[derive(Debug, Clone)]
struct PhdrRecord {
    name: String,
    preset: u16,
    bank: u16,
    bag_index: u16,
}

#[derive(Debug, Clone, Copy)]
struct BagRecord {
    gen_index: u16,
}

#[derive(Debug, Clone, Copy)]
struct GenRecord {
    oper: u16,
    amount: i16,
}

impl GenRecord {
    fn amount_range(&self) -> (u8, u8) {
        let lo = (self.amount & 0xFF) as u8;
        let hi = ((self.amount >> 8) & 0xFF) as u8;
        (lo, hi)
    }
}

#[derive(Debug, Clone)]
struct InstRecord {
    #[allow(dead_code)]
    name: String,
    bag_index: u16,
}

#[derive(Debug, Clone)]
struct ShdrRecord {
    name: String,
    start: u32,
    end: u32,
    loop_start: u32,
    loop_end: u32,
    sample_rate: u32,
    original_pitch: u8,
    sample_type: u16,
}

// ── Low-level reading helpers ───────────────────────────────────────────

fn read_u8(data: &[u8], offset: usize) -> Result<u8> {
    data.get(offset).copied().ok_or_else(|| {
        NidhiError::ImportError(format!("unexpected end of data at offset {offset}"))
    })
}

fn read_u16_le(data: &[u8], offset: usize) -> Result<u16> {
    if offset + 2 > data.len() {
        return Err(NidhiError::ImportError(format!(
            "unexpected end of data at offset {offset}"
        )));
    }
    Ok(u16::from_le_bytes([data[offset], data[offset + 1]]))
}

fn read_i16_le(data: &[u8], offset: usize) -> Result<i16> {
    read_u16_le(data, offset).map(|v| v as i16)
}

fn read_u32_le(data: &[u8], offset: usize) -> Result<u32> {
    if offset + 4 > data.len() {
        return Err(NidhiError::ImportError(format!(
            "unexpected end of data at offset {offset}"
        )));
    }
    Ok(u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]))
}

fn read_fourcc(data: &[u8], offset: usize) -> Result<[u8; 4]> {
    if offset + 4 > data.len() {
        return Err(NidhiError::ImportError(format!(
            "unexpected end of data at offset {offset}"
        )));
    }
    let mut cc = [0u8; 4];
    cc.copy_from_slice(&data[offset..offset + 4]);
    Ok(cc)
}

fn read_fixed_string(data: &[u8], offset: usize, len: usize) -> Result<String> {
    if offset + len > data.len() {
        return Err(NidhiError::ImportError(format!(
            "unexpected end of data at offset {offset}"
        )));
    }
    let slice = &data[offset..offset + len];
    let end = slice.iter().position(|&b| b == 0).unwrap_or(len);
    Ok(String::from_utf8_lossy(&slice[..end]).into())
}

// ── Chunk iteration ─────────────────────────────────────────────────────

struct Chunk<'a> {
    id: [u8; 4],
    data: &'a [u8],
}

struct ChunkIter<'a> {
    data: &'a [u8],
    offset: usize,
}

fn iter_chunks(data: &[u8]) -> ChunkIter<'_> {
    ChunkIter { data, offset: 0 }
}

impl<'a> Iterator for ChunkIter<'a> {
    type Item = Result<Chunk<'a>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset + 8 > self.data.len() {
            return None;
        }
        let id = match read_fourcc(self.data, self.offset) {
            Ok(cc) => cc,
            Err(e) => return Some(Err(e)),
        };
        let size = match read_u32_le(self.data, self.offset + 4) {
            Ok(s) => s as usize,
            Err(e) => return Some(Err(e)),
        };
        let data_start = self.offset + 8;
        let data_end = data_start + size;
        if data_end > self.data.len() {
            self.offset = self.data.len();
            return Some(Err(NidhiError::ImportError(format!(
                "chunk extends beyond data at offset {}",
                self.offset
            ))));
        }
        let chunk = Chunk {
            id,
            data: &self.data[data_start..data_end],
        };
        self.offset = data_end + (size & 1); // pad to even
        Some(Ok(chunk))
    }
}

// ── Record parsers ──────────────────────────────────────────────────────

fn parse_phdr_records(data: &[u8]) -> Result<Vec<PhdrRecord>> {
    const SIZE: usize = 38;
    let count = data.len() / SIZE;
    let mut records = Vec::with_capacity(count);
    for i in 0..count {
        let off = i * SIZE;
        records.push(PhdrRecord {
            name: read_fixed_string(data, off, 20)?,
            preset: read_u16_le(data, off + 20)?,
            bank: read_u16_le(data, off + 22)?,
            bag_index: read_u16_le(data, off + 24)?,
        });
    }
    Ok(records)
}

fn parse_bag_records(data: &[u8]) -> Result<Vec<BagRecord>> {
    const SIZE: usize = 4;
    let count = data.len() / SIZE;
    let mut records = Vec::with_capacity(count);
    for i in 0..count {
        let off = i * SIZE;
        records.push(BagRecord {
            gen_index: read_u16_le(data, off)?,
        });
    }
    Ok(records)
}

fn parse_gen_records(data: &[u8]) -> Result<Vec<GenRecord>> {
    const SIZE: usize = 4;
    let count = data.len() / SIZE;
    let mut records = Vec::with_capacity(count);
    for i in 0..count {
        let off = i * SIZE;
        records.push(GenRecord {
            oper: read_u16_le(data, off)?,
            amount: read_i16_le(data, off + 2)?,
        });
    }
    Ok(records)
}

fn parse_inst_records(data: &[u8]) -> Result<Vec<InstRecord>> {
    const SIZE: usize = 22;
    let count = data.len() / SIZE;
    let mut records = Vec::with_capacity(count);
    for i in 0..count {
        let off = i * SIZE;
        records.push(InstRecord {
            name: read_fixed_string(data, off, 20)?,
            bag_index: read_u16_le(data, off + 20)?,
        });
    }
    Ok(records)
}

fn parse_shdr_records(data: &[u8]) -> Result<Vec<ShdrRecord>> {
    const SIZE: usize = 46;
    let count = data.len() / SIZE;
    let mut records = Vec::with_capacity(count);
    for i in 0..count {
        let off = i * SIZE;
        records.push(ShdrRecord {
            name: read_fixed_string(data, off, 20)?,
            start: read_u32_le(data, off + 20)?,
            end: read_u32_le(data, off + 24)?,
            loop_start: read_u32_le(data, off + 28)?,
            loop_end: read_u32_le(data, off + 32)?,
            sample_rate: read_u32_le(data, off + 36)?,
            original_pitch: read_u8(data, off + 40)?,
            sample_type: read_u16_le(data, off + 44)?,
        });
    }
    Ok(records)
}

// ── Sample extraction ───────────────────────────────────────────────────

fn pcm16_to_f32(data: &[u8], start_sample: usize, end_sample: usize) -> Vec<f32> {
    let byte_start = start_sample * 2;
    let byte_end = end_sample * 2;
    if byte_end > data.len() || byte_start > byte_end {
        return Vec::new();
    }
    let slice = &data[byte_start..byte_end];
    let num_samples = (byte_end - byte_start) / 2;
    let mut out = Vec::with_capacity(num_samples);
    for i in 0..num_samples {
        let off = i * 2;
        let sample = i16::from_le_bytes([slice[off], slice[off + 1]]);
        out.push(sample as f32 / 32768.0);
    }
    out
}

// ── Public API ──────────────────────────────────────────────────────────

/// Parse an SF2 (SoundFont 2) file from raw bytes.
///
/// Returns a tuple of:
/// - `Vec<Sf2Preset>` — preset metadata (name, bank, program number)
/// - `Vec<Instrument>` — one instrument per preset, containing zones
/// - `SampleBank` — all extracted samples
///
/// The preset and instrument vectors are parallel (same indices).
/// Each zone's `SampleId` references a sample in the returned `SampleBank`.
pub fn parse(data: &[u8]) -> Result<(Vec<Sf2Preset>, Vec<Instrument>, SampleBank)> {
    // 1. Validate RIFF/sfbk header
    if data.len() < 12 {
        return Err(NidhiError::ImportError(
            "file too small to be a valid SF2".into(),
        ));
    }
    let riff_id = read_fourcc(data, 0)?;
    if riff_id != RIFF_ID {
        return Err(NidhiError::ImportError("not a RIFF file".into()));
    }
    let form_type = read_fourcc(data, 8)?;
    if form_type != SFBK_ID {
        return Err(NidhiError::ImportError(format!(
            "RIFF form type is {:?}, expected 'sfbk'",
            String::from_utf8_lossy(&form_type)
        )));
    }

    // 2. Find sdta (smpl) and pdta LIST chunks
    let mut sdta_smpl: Option<&[u8]> = None;
    let mut pdta: Option<&[u8]> = None;

    for chunk in iter_chunks(&data[12..]) {
        let chunk = chunk?;
        if chunk.id == LIST_ID && chunk.data.len() >= 4 {
            let list_type = read_fourcc(chunk.data, 0)?;
            match &list_type {
                b"sdta" => {
                    for sub in iter_chunks(&chunk.data[4..]) {
                        let sub = sub?;
                        if &sub.id == b"smpl" {
                            sdta_smpl = Some(sub.data);
                        }
                    }
                }
                b"pdta" => pdta = Some(&chunk.data[4..]),
                _ => {}
            }
        }
    }

    let smpl_data =
        sdta_smpl.ok_or_else(|| NidhiError::ImportError("missing sdta/smpl chunk".into()))?;
    let pdta_data = pdta.ok_or_else(|| NidhiError::ImportError("missing pdta chunk".into()))?;

    // 3. Parse pdta sub-chunks
    let mut phdr_raw: Option<&[u8]> = None;
    let mut pbag_raw: Option<&[u8]> = None;
    let mut pgen_raw: Option<&[u8]> = None;
    let mut inst_raw: Option<&[u8]> = None;
    let mut ibag_raw: Option<&[u8]> = None;
    let mut igen_raw: Option<&[u8]> = None;
    let mut shdr_raw: Option<&[u8]> = None;

    for chunk in iter_chunks(pdta_data) {
        let chunk = chunk?;
        match chunk.id {
            id if id == PHDR_ID => phdr_raw = Some(chunk.data),
            id if id == PBAG_ID => pbag_raw = Some(chunk.data),
            id if id == PGEN_ID => pgen_raw = Some(chunk.data),
            id if id == INST_ID => inst_raw = Some(chunk.data),
            id if id == IBAG_ID => ibag_raw = Some(chunk.data),
            id if id == IGEN_ID => igen_raw = Some(chunk.data),
            id if id == SHDR_ID => shdr_raw = Some(chunk.data),
            _ => {}
        }
    }

    let phdrs = parse_phdr_records(
        phdr_raw.ok_or_else(|| NidhiError::ImportError("missing phdr".into()))?,
    )?;
    let pbags =
        parse_bag_records(pbag_raw.ok_or_else(|| NidhiError::ImportError("missing pbag".into()))?)?;
    let pgens =
        parse_gen_records(pgen_raw.ok_or_else(|| NidhiError::ImportError("missing pgen".into()))?)?;
    let insts = parse_inst_records(
        inst_raw.ok_or_else(|| NidhiError::ImportError("missing inst".into()))?,
    )?;
    let ibags =
        parse_bag_records(ibag_raw.ok_or_else(|| NidhiError::ImportError("missing ibag".into()))?)?;
    let igens =
        parse_gen_records(igen_raw.ok_or_else(|| NidhiError::ImportError("missing igen".into()))?)?;
    let shdrs = parse_shdr_records(
        shdr_raw.ok_or_else(|| NidhiError::ImportError("missing shdr".into()))?,
    )?;

    // 4. Resolve presets → instruments → sample zones
    let mut presets = Vec::new();
    let mut instruments = Vec::new();
    let mut bank = SampleBank::new();

    for pi in 0..phdrs.len().saturating_sub(1) {
        let phdr = &phdrs[pi];
        let bag_start = phdr.bag_index as usize;
        let bag_end = phdrs[pi + 1].bag_index as usize;

        let mut inst_obj = Instrument::new(&phdr.name);

        for bi in bag_start..bag_end {
            if bi >= pbags.len() {
                break;
            }
            let gen_start = pbags[bi].gen_index as usize;
            let gen_end = if bi + 1 < pbags.len() {
                pbags[bi + 1].gen_index as usize
            } else {
                pgens.len()
            };

            let mut inst_index: Option<usize> = None;
            let mut preset_key_range: Option<(u8, u8)> = None;
            let mut preset_vel_range: Option<(u8, u8)> = None;

            for pg in &pgens[gen_start..gen_end.min(pgens.len())] {
                match pg.oper {
                    GEN_INSTRUMENT => inst_index = Some(pg.amount as usize),
                    GEN_KEY_RANGE => preset_key_range = Some(pg.amount_range()),
                    GEN_VEL_RANGE => preset_vel_range = Some(pg.amount_range()),
                    _ => {}
                }
            }

            let Some(ii) = inst_index else { continue };
            if ii >= insts.len().saturating_sub(1) {
                continue;
            }

            let inst_rec = &insts[ii];
            let ibag_start = inst_rec.bag_index as usize;
            let ibag_end = insts[ii + 1].bag_index as usize;

            for ib in ibag_start..ibag_end {
                if ib >= ibags.len() {
                    break;
                }
                let igen_start = ibags[ib].gen_index as usize;
                let igen_end = if ib + 1 < ibags.len() {
                    ibags[ib + 1].gen_index as usize
                } else {
                    igens.len()
                };

                let mut sample_id: Option<usize> = None;
                let mut key_range: (u8, u8) = (0, 127);
                let mut vel_range: (u8, u8) = (0, 127);
                let mut root_key_override: Option<u8> = None;
                let mut sample_modes: u16 = 0;

                for ig in &igens[igen_start..igen_end.min(igens.len())] {
                    match ig.oper {
                        GEN_SAMPLE_ID => sample_id = Some(ig.amount as usize),
                        GEN_KEY_RANGE => key_range = ig.amount_range(),
                        GEN_VEL_RANGE => vel_range = ig.amount_range(),
                        GEN_OVERRIDING_ROOT_KEY => {
                            let k = ig.amount as u8;
                            if k <= 127 {
                                root_key_override = Some(k);
                            }
                        }
                        GEN_SAMPLE_MODES => sample_modes = ig.amount as u16,
                        _ => {}
                    }
                }

                let Some(sid) = sample_id else { continue };
                if sid >= shdrs.len().saturating_sub(1) {
                    continue;
                }
                let shdr = &shdrs[sid];

                // Skip ROM samples
                if shdr.sample_type & 0x8000 != 0 {
                    continue;
                }

                let root_key = root_key_override.unwrap_or(shdr.original_pitch);
                let loop_mode = match sample_modes & 3 {
                    0 => LoopMode::OneShot,
                    1 | 2 => LoopMode::Forward,
                    3 => LoopMode::LoopSustain,
                    _ => LoopMode::OneShot,
                };

                // Apply preset-level range restriction
                let final_key = if let Some(pk) = preset_key_range {
                    (key_range.0.max(pk.0), key_range.1.min(pk.1))
                } else {
                    key_range
                };
                let final_vel = if let Some(pv) = preset_vel_range {
                    (vel_range.0.max(pv.0), vel_range.1.min(pv.1))
                } else {
                    vel_range
                };

                // Extract PCM and add to bank
                let pcm = pcm16_to_f32(smpl_data, shdr.start as usize, shdr.end as usize);
                let sample = Sample::from_mono(pcm, shdr.sample_rate).with_name(&shdr.name);
                let sample_bank_id = bank.add(sample);

                // Build zone
                let mut zone = Zone::new(sample_bank_id)
                    .with_key_range(final_key.0, final_key.1)
                    .with_vel_range(final_vel.0, final_vel.1)
                    .with_root_note(root_key);

                if loop_mode != LoopMode::OneShot {
                    let ls = shdr.loop_start.saturating_sub(shdr.start) as usize;
                    let le = shdr.loop_end.saturating_sub(shdr.start) as usize;
                    zone = zone.with_loop(loop_mode, ls, le);
                }

                inst_obj.add_zone(zone);
            }
        }

        if inst_obj.zone_count() > 0 {
            presets.push(Sf2Preset {
                name: phdr.name.clone(),
                bank: phdr.bank,
                preset_number: phdr.preset,
            });
            instruments.push(inst_obj);
        }
    }

    Ok((presets, instruments, bank))
}

// ── Test helpers ────────────────────────────────────────────────────────

#[cfg(test)]
mod test_helpers {
    use alloc::vec::Vec;

    pub fn write_u16_le(buf: &mut Vec<u8>, v: u16) {
        buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn write_u32_le(buf: &mut Vec<u8>, v: u32) {
        buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn write_i16_le(buf: &mut Vec<u8>, v: i16) {
        buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn write_fourcc(buf: &mut Vec<u8>, cc: &[u8; 4]) {
        buf.extend_from_slice(cc);
    }

    pub fn write_fixed_string(buf: &mut Vec<u8>, s: &str, len: usize) {
        let bytes = s.as_bytes();
        let copy_len = bytes.len().min(len);
        buf.extend_from_slice(&bytes[..copy_len]);
        for _ in copy_len..len {
            buf.push(0);
        }
    }

    pub fn make_chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut buf = Vec::new();
        write_fourcc(&mut buf, id);
        write_u32_le(&mut buf, data.len() as u32);
        buf.extend_from_slice(data);
        if !data.len().is_multiple_of(2) {
            buf.push(0);
        }
        buf
    }

    pub fn make_list(form_type: &[u8; 4], sub_chunks: &[Vec<u8>]) -> Vec<u8> {
        let mut inner = Vec::new();
        inner.extend_from_slice(form_type);
        for sc in sub_chunks {
            inner.extend_from_slice(sc);
        }
        let mut buf = Vec::new();
        write_fourcc(&mut buf, b"LIST");
        write_u32_le(&mut buf, inner.len() as u32);
        buf.extend_from_slice(&inner);
        buf
    }

    pub fn make_sf2(info: Vec<u8>, sdta: Vec<u8>, pdta: Vec<u8>) -> Vec<u8> {
        let mut inner = Vec::new();
        inner.extend_from_slice(b"sfbk");
        inner.extend_from_slice(&info);
        inner.extend_from_slice(&sdta);
        inner.extend_from_slice(&pdta);
        let mut buf = Vec::new();
        write_fourcc(&mut buf, b"RIFF");
        write_u32_le(&mut buf, inner.len() as u32);
        buf.extend_from_slice(&inner);
        buf
    }

    pub fn make_phdr(name: &str, preset: u16, bank: u16, bag_ndx: u16) -> Vec<u8> {
        let mut buf = Vec::new();
        write_fixed_string(&mut buf, name, 20);
        write_u16_le(&mut buf, preset);
        write_u16_le(&mut buf, bank);
        write_u16_le(&mut buf, bag_ndx);
        write_u32_le(&mut buf, 0); // library
        write_u32_le(&mut buf, 0); // genre
        write_u32_le(&mut buf, 0); // morphology
        buf
    }

    pub fn make_bag(gen_ndx: u16, mod_ndx: u16) -> Vec<u8> {
        let mut buf = Vec::new();
        write_u16_le(&mut buf, gen_ndx);
        write_u16_le(&mut buf, mod_ndx);
        buf
    }

    pub fn make_gen(oper: u16, amount: i16) -> Vec<u8> {
        let mut buf = Vec::new();
        write_u16_le(&mut buf, oper);
        write_i16_le(&mut buf, amount);
        buf
    }

    pub fn make_gen_range(oper: u16, lo: u8, hi: u8) -> Vec<u8> {
        let amount = (lo as i16) | ((hi as i16) << 8);
        make_gen(oper, amount)
    }

    pub fn make_inst(name: &str, bag_ndx: u16) -> Vec<u8> {
        let mut buf = Vec::new();
        write_fixed_string(&mut buf, name, 20);
        write_u16_le(&mut buf, bag_ndx);
        buf
    }

    #[allow(clippy::too_many_arguments)]
    pub fn make_shdr(
        name: &str,
        start: u32,
        end: u32,
        loop_start: u32,
        loop_end: u32,
        sample_rate: u32,
        original_pitch: u8,
        sample_type: u16,
    ) -> Vec<u8> {
        let mut buf = Vec::new();
        write_fixed_string(&mut buf, name, 20);
        write_u32_le(&mut buf, start);
        write_u32_le(&mut buf, end);
        write_u32_le(&mut buf, loop_start);
        write_u32_le(&mut buf, loop_end);
        write_u32_le(&mut buf, sample_rate);
        buf.push(original_pitch);
        buf.push(0); // pitch correction
        write_u16_le(&mut buf, 0); // sample link
        write_u16_le(&mut buf, sample_type);
        buf
    }

    pub fn make_pcm16(samples: &[f32]) -> Vec<u8> {
        let mut buf = Vec::with_capacity(samples.len() * 2);
        for &s in samples {
            let val = (s * 32767.0).round().clamp(-32768.0, 32767.0) as i16;
            buf.extend_from_slice(&val.to_le_bytes());
        }
        buf
    }

    #[allow(clippy::too_many_arguments)]
    pub fn build_minimal_sf2(
        preset_name: &str,
        sample_data: &[f32],
        root_key: u8,
        key_lo: u8,
        key_hi: u8,
        vel_lo: u8,
        vel_hi: u8,
        loop_mode: u16,
        loop_start: u32,
        loop_end: u32,
    ) -> Vec<u8> {
        let num_samples = sample_data.len() as u32;
        let pcm = make_pcm16(sample_data);

        let ifil = {
            let mut d = Vec::new();
            write_u16_le(&mut d, 2);
            write_u16_le(&mut d, 1);
            make_chunk(b"ifil", &d)
        };
        let info_list = make_list(b"INFO", &[ifil]);
        let sdta_list = make_list(b"sdta", &[make_chunk(b"smpl", &pcm)]);

        let mut phdr_buf = Vec::new();
        phdr_buf.extend_from_slice(&make_phdr(preset_name, 0, 0, 0));
        phdr_buf.extend_from_slice(&make_phdr("EOP", 0, 0, 1));

        let mut pbag_buf = Vec::new();
        pbag_buf.extend_from_slice(&make_bag(0, 0));
        pbag_buf.extend_from_slice(&make_bag(1, 0));

        let mut pgen_buf = Vec::new();
        pgen_buf.extend_from_slice(&make_gen(41, 0));
        pgen_buf.extend_from_slice(&make_gen(0, 0));

        let mut inst_buf = Vec::new();
        inst_buf.extend_from_slice(&make_inst("Inst", 0));
        inst_buf.extend_from_slice(&make_inst("EOI", 1));

        let mut ibag_buf = Vec::new();
        ibag_buf.extend_from_slice(&make_bag(0, 0));
        ibag_buf.extend_from_slice(&make_bag(4, 0));

        let mut igen_buf = Vec::new();
        igen_buf.extend_from_slice(&make_gen_range(43, key_lo, key_hi));
        igen_buf.extend_from_slice(&make_gen_range(44, vel_lo, vel_hi));
        igen_buf.extend_from_slice(&make_gen(54, loop_mode as i16));
        igen_buf.extend_from_slice(&make_gen(53, 0));
        igen_buf.extend_from_slice(&make_gen(0, 0));

        let mut shdr_buf = Vec::new();
        shdr_buf.extend_from_slice(&make_shdr(
            "Sample",
            0,
            num_samples,
            loop_start,
            loop_end,
            44100,
            root_key,
            1,
        ));
        shdr_buf.extend_from_slice(&make_shdr("EOS", 0, 0, 0, 0, 0, 0, 0));

        let pdta_list = make_list(
            b"pdta",
            &[
                make_chunk(b"phdr", &phdr_buf),
                make_chunk(b"pbag", &pbag_buf),
                make_chunk(b"pmod", &[]),
                make_chunk(b"pgen", &pgen_buf),
                make_chunk(b"inst", &inst_buf),
                make_chunk(b"ibag", &ibag_buf),
                make_chunk(b"imod", &[]),
                make_chunk(b"igen", &igen_buf),
                make_chunk(b"shdr", &shdr_buf),
            ],
        );

        make_sf2(info_list, sdta_list, pdta_list)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sample::SampleId;
    use test_helpers::*;

    #[test]
    fn reject_too_small() {
        assert!(parse(&[0; 4]).is_err());
    }

    #[test]
    fn reject_non_riff() {
        let mut data = [0u8; 12];
        data[0..4].copy_from_slice(b"NOTF");
        assert!(parse(&data).is_err());
    }

    #[test]
    fn reject_wrong_form_type() {
        let mut data = [0u8; 12];
        data[0..4].copy_from_slice(b"RIFF");
        data[4..8].copy_from_slice(&4u32.to_le_bytes());
        data[8..12].copy_from_slice(b"WAVE");
        assert!(parse(&data).is_err());
    }

    #[test]
    fn parse_minimal_sf2() {
        let samples: Vec<f32> = (0..100).map(|i| (i as f32 / 100.0) * 2.0 - 1.0).collect();
        let sf2 = build_minimal_sf2("Piano", &samples, 60, 36, 84, 0, 127, 0, 0, 0);

        let (presets, instruments, bank) = parse(&sf2).unwrap();
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].name, "Piano");
        assert_eq!(instruments.len(), 1);
        assert_eq!(instruments[0].zone_count(), 1);

        let zone = &instruments[0].zones()[0];
        assert_eq!(zone.key_lo, 36);
        assert_eq!(zone.key_hi, 84);
        assert_eq!(zone.root_note, 60);
        assert_eq!(zone.loop_mode, LoopMode::OneShot);

        assert_eq!(bank.len(), 1);
        assert_eq!(bank.get(SampleId(0)).unwrap().frames(), 100);
    }

    #[test]
    fn pcm16_conversion_accuracy() {
        let samples = [0.0f32, 0.5, -0.5, 1.0, -1.0];
        let pcm = make_pcm16(&samples);
        let converted = pcm16_to_f32(&pcm, 0, samples.len());
        for (orig, conv) in samples.iter().zip(converted.iter()) {
            assert!((orig - conv).abs() < 0.001, "expected ~{orig}, got {conv}");
        }
    }

    #[test]
    fn forward_loop_preserved() {
        let samples = vec![0.0f32; 200];
        let sf2 = build_minimal_sf2("Loop", &samples, 60, 0, 127, 0, 127, 1, 50, 150);

        let (_, instruments, _) = parse(&sf2).unwrap();
        let zone = &instruments[0].zones()[0];
        assert_eq!(zone.loop_mode, LoopMode::Forward);
        assert_eq!(zone.loop_start, 50);
        assert_eq!(zone.loop_end, 150);
    }

    #[test]
    fn loop_sustain_mode_3() {
        let samples = vec![0.0f32; 200];
        let sf2 = build_minimal_sf2("Sustain", &samples, 60, 0, 127, 0, 127, 3, 50, 150);

        let (_, instruments, _) = parse(&sf2).unwrap();
        let zone = &instruments[0].zones()[0];
        assert_eq!(zone.loop_mode, LoopMode::LoopSustain);
    }

    #[test]
    fn velocity_range_preserved() {
        let samples = vec![0.0f32; 50];
        let sf2 = build_minimal_sf2("Vel", &samples, 60, 0, 127, 32, 96, 0, 0, 0);

        let (_, instruments, _) = parse(&sf2).unwrap();
        let zone = &instruments[0].zones()[0];
        assert_eq!(zone.vel_lo, 32);
        assert_eq!(zone.vel_hi, 96);
    }

    #[test]
    fn pcm16_empty() {
        assert!(pcm16_to_f32(&[], 0, 0).is_empty());
    }

    #[test]
    fn pcm16_out_of_bounds() {
        assert!(pcm16_to_f32(&[0, 0], 0, 100).is_empty());
    }
}
