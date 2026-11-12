//! Bounded storage decoder for observed appearance-profile tails (247 to 259 bytes).
//! Offsets begin immediately after the profile's counted UTF-16 name. Phase slots and
//! flag bytes retain storage identity: rendering precedence, flag semantics, interpolation,
//! and preset direction vectors are not inferred by this decoder.
use crate::FormatError;
use serde::{Deserialize, Serialize};

/// Size when no phase enables transparency interpolation.
pub const TAIL_BYTES: usize = 247;
pub const MAX_TAIL_BYTES: usize = 259;
pub const PHASE_BYTES: usize = 63;
/// Phase starts in the non-interpolating layout; use each decoded tail_offset otherwise.
pub const PHASE_OFFSETS: [usize; 3] = [6, 69, 176];
const BLOCK_OPEN: [u8; 4] = [0x10, 0x48, 0x69, 0x23];
const BLOCK_CLOSE: [u8; 4] = [0x6c, 0x4a, 0x92, 0x90];
const LIST_OPEN: [u8; 4] = [0x7a, 0x54, 0xaf, 0x39];
const LIST_CLOSE: [u8; 4] = [0x16, 0xe4, 0x99, 0x6f];
const TAIL_CLOSE: [u8; 4] = [0x3d, 0xc8, 0xb0, 0x03];
type Result<T> = std::result::Result<T, FormatError>;

/// Only code 1 has independently observed native Action readback in the admitted corpus.
/// Other numbers remain explicit, even if a retained profile name suggests a meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mapping", content = "storage_code", rename_all = "snake_case")]
pub enum ActionCode {
    ObservedInstall,
    Unknown(u32),
}
impl ActionCode {
    fn from_storage(code: u32) -> Self {
        if code == 1 { Self::ObservedInstall } else { Self::Unknown(code) }
    }
}
/// Code mappings are observations in build 6.5.5.28, not a cross-version enum contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mapping", content = "storage_code", rename_all = "snake_case")]
pub enum GrowthCode {
    ObservedNoGrowth,
    ObservedBottomToTop,
    ObservedLeftToRight,
    Unknown(u32),
}
impl GrowthCode {
    fn from_storage(code: u32) -> Self {
        match code {
            1 => Self::ObservedNoGrowth,
            2 => Self::ObservedBottomToTop,
            4 => Self::ObservedLeftToRight,
            other => Self::Unknown(other),
        }
    }
}
/// Slot labels deliberately do not assign active/before/after temporal precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PhaseSlot {
    First,
    Second,
    Third,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhaseBlock {
    pub slot: PhaseSlot,
    pub tail_offset: usize,
    pub byte_length: usize,
    /// Bytes at block +4 and +5. These are not assumed to be booleans.
    pub leading_flags_storage: [u8; 2],
    pub rgb_storage: [f32; 3],
    pub opacity_percent_storage: u32,
    /// Present only when structural interpolation flag at block +24 is 1.
    pub finish_opacity_percent_storage: Option<u32>,
    /// Bytes at block +22, +23 and +24; semantics remain unresolved.
    pub effect_flags_storage: [u8; 3],
    pub growth_storage: u32,
    pub growth_mapping: GrowthCode,
    /// Raw finite vector at block +29 (+4 with interpolation). Preset types may leave it stale.
    pub custom_direction_storage: [f64; 3],
    /// Six bytes at block +53 (+4 with interpolation), retained without reinterpretation.
    pub trailing_flags_storage: [u8; 6],
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileTail {
    pub schema_version: u32,
    pub byte_length: usize,
    pub header_flags_storage: [u8; 2],
    pub action_storage: u32,
    pub action_mapping: ActionCode,
    pub phases: [PhaseBlock; 3],
    /// The admitted layout has an empty list between the second and third blocks.
    pub extension_count_storage: u32,
    /// Sixteen reference bytes after the empty list; no identity semantics are claimed.
    pub inter_phase_reference_storage: [u8; 16],
    /// Sixteen opaque bytes after the reference storage.
    pub inter_phase_opaque_storage: [u8; 16],
    /// Final ordinal (baseline tail +243), distinct from Action storage at tail +2.
    pub ordinal_storage: u32,
}
fn range(raw: &[u8], at: usize, size: usize) -> Result<&[u8]> {
    let end = at.checked_add(size).ok_or_else(|| FormatError::TooLarge("appearance offset arithmetic".into()))?;
    raw.get(at..end).ok_or_else(|| FormatError::Truncated("appearance field".into()))
}
fn array<const N: usize>(raw: &[u8], at: usize) -> Result<[u8; N]> {
    range(raw, at, N)?.try_into().map_err(|_| FormatError::Truncated("appearance fixed field".into()))
}
fn integer(raw: &[u8], at: usize) -> Result<u32> {
    Ok(u32::from_le_bytes(array(raw, at)?))
}
fn marker(raw: &[u8], at: usize, expected: &[u8; 4]) -> Result<()> {
    if range(raw, at, 4)? != expected {
        return Err(FormatError::Unsupported(format!("appearance framing at tail offset {at}")));
    }
    Ok(())
}
fn phase(raw: &[u8], at: usize, slot: PhaseSlot) -> Result<PhaseBlock> {
    let prefix = range(raw, at, 25)?;
    marker(prefix, 0, &BLOCK_OPEN)?;
    let effect_flags_storage: [u8; 3] = array(prefix, 22)?;
    let interpolation = effect_flags_storage.get(2).copied().ok_or_else(|| FormatError::Truncated("appearance interpolation flag".into()))?;
    let extra = match interpolation {
        0 => 0usize,
        1 => 4,
        _ => return Err(FormatError::Unsupported("unknown appearance interpolation layout flag".into())),
    };
    let byte_length = PHASE_BYTES + extra;
    let block = range(raw, at, byte_length)?;
    marker(block, 59 + extra, &BLOCK_CLOSE)?;
    let mut rgb = [0.; 3];
    for (axis, value) in rgb.iter_mut().enumerate() {
        *value = f32::from_le_bytes(array(block, 6 + axis * 4)?);
        if !value.is_finite() || !(0.0..=1.0).contains(value) {
            return Err(FormatError::Malformed("appearance RGB must be finite and within 0..1".into()));
        }
    }
    let start_opacity = opacity_value(block, 18)?;
    let finish_opacity = if extra == 4 { Some(opacity_value(block, 25)?) } else { None };
    let growth = integer(block, 25 + extra)?;
    let mut direction = [0.; 3];
    for (axis, value) in direction.iter_mut().enumerate() {
        *value = f64::from_le_bytes(array(block, 29 + extra + axis * 8)?);
        if !value.is_finite() {
            return Err(FormatError::Malformed("appearance custom direction is not finite".into()));
        }
    }
    Ok(PhaseBlock {
        slot,
        tail_offset: at,
        byte_length,
        leading_flags_storage: array(block, 4)?,
        rgb_storage: rgb,
        opacity_percent_storage: start_opacity,
        finish_opacity_percent_storage: finish_opacity,
        effect_flags_storage,
        growth_storage: growth,
        growth_mapping: GrowthCode::from_storage(growth),
        custom_direction_storage: direction,
        trailing_flags_storage: array(block, 53 + extra)?,
    })
}
fn opacity_value(raw: &[u8], at: usize) -> Result<u32> {
    let value = integer(raw, at)?;
    if value > 100 {
        return Err(FormatError::Unsupported("appearance opacity storage exceeds the observed 0..100 range".into()));
    }
    Ok(value)
}
/// Decode one framed profile tail from a containing section, examining at most 259 bytes.
/// Returns the typed storage followed by its exact consumed byte count. The caller may continue
/// at the next record; unknown structural interpolation flags are never treated as false.
pub fn parse_prefix(raw: &[u8]) -> Result<(ProfileTail, usize)> {
    if raw.len() < TAIL_BYTES {
        return Err(FormatError::Truncated("appearance tail requires at least 247 bytes".into()));
    }
    let first = phase(raw, 6, PhaseSlot::First)?;
    let second = phase(raw, 6 + first.byte_length, PhaseSlot::Second)?;
    let gap = second.tail_offset + second.byte_length;
    marker(raw, gap, &LIST_OPEN)?;
    let extension_count = integer(raw, gap + 4)?;
    if extension_count != 0 {
        return Err(FormatError::Unsupported("appearance extension list is not empty".into()));
    }
    marker(raw, gap + 8, &LIST_CLOSE)?;
    let third = phase(raw, gap + 44, PhaseSlot::Third)?;
    let footer = third.tail_offset + third.byte_length;
    marker(raw, footer, &TAIL_CLOSE)?;
    let ordinal = integer(raw, footer + 4)?;
    let consumed = footer + 8;
    if consumed > MAX_TAIL_BYTES {
        return Err(FormatError::TooLarge("appearance tail exceeds 259 bytes".into()));
    }
    let action = integer(raw, 2)?;
    Ok((
        ProfileTail {
            schema_version: 1,
            byte_length: consumed,
            header_flags_storage: array(raw, 0)?,
            action_storage: action,
            action_mapping: ActionCode::from_storage(action),
            phases: [first, second, third],
            extension_count_storage: extension_count,
            inter_phase_reference_storage: array(raw, gap + 12)?,
            inter_phase_opaque_storage: array(raw, gap + 28)?,
            ordinal_storage: ordinal,
        },
        consumed,
    ))
}
/// Decode an exact observed tail; rejects trailing bytes rather than hiding another layout.
pub fn parse_exact(raw: &[u8]) -> Result<ProfileTail> {
    if raw.len() > MAX_TAIL_BYTES {
        return Err(FormatError::Unsupported("appearance tail exceeds the admitted layout".into()));
    }
    let (profile, consumed) = parse_prefix(raw)?;
    if consumed != raw.len() {
        return Err(FormatError::Unsupported("trailing appearance tail bytes".into()));
    }
    Ok(profile)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn put(raw: &mut [u8], at: usize, value: &[u8]) {
        if let Some(dest) = raw.get_mut(at..at + value.len()) {
            dest.copy_from_slice(value);
        }
    }
    fn fixture() -> Vec<u8> {
        let mut raw = vec![0; TAIL_BYTES];
        put(&mut raw, 0, &[9, 8]);
        put(&mut raw, 2, &1u32.to_le_bytes());
        for (index, at) in PHASE_OFFSETS.into_iter().enumerate() {
            put(&mut raw, at, &BLOCK_OPEN);
            put(&mut raw, at + 4, &[7, 6]);
            for (axis, v) in [0.25f32, 0.5, 0.75].into_iter().enumerate() {
                put(&mut raw, at + 6 + axis * 4, &v.to_le_bytes());
            }
            put(&mut raw, at + 18, &(60u32 + index as u32).to_le_bytes());
            put(&mut raw, at + 22, &[5, 4, 0]);
            put(&mut raw, at + 25, &(4u32 + index as u32).to_le_bytes());
            for (axis, v) in [3f64, 4., 5.].into_iter().enumerate() {
                put(&mut raw, at + 29 + axis * 8, &v.to_le_bytes());
            }
            put(&mut raw, at + 53, &[1, 2, 3, 4, 5, 6]);
            put(&mut raw, at + 59, &BLOCK_CLOSE);
        }
        put(&mut raw, 132, &LIST_OPEN);
        put(&mut raw, 140, &LIST_CLOSE);
        put(&mut raw, 144, &[10; 16]);
        put(&mut raw, 160, &[11; 16]);
        put(&mut raw, 239, &TAIL_CLOSE);
        put(&mut raw, 243, &99u32.to_le_bytes());
        raw
    }
    #[test]
    fn action_ordinal_and_unaligned_growth_offsets_are_distinct() -> Result<()> {
        let raw = fixture();
        let tail = parse_exact(&raw)?;
        assert_eq!(tail.action_storage, 1);
        assert_eq!(tail.action_mapping, ActionCode::ObservedInstall);
        assert_eq!(tail.ordinal_storage, 99);
        let first = tail.phases.first().ok_or_else(|| FormatError::Malformed("test phase".into()))?;
        assert_eq!(first.growth_storage, 4);
        assert_ne!(integer(&raw, 30)?, 4);
        assert_eq!(first.opacity_percent_storage, 60);
        assert_eq!(first.custom_direction_storage, [3., 4., 5.]);
        assert_eq!(first.growth_mapping, GrowthCode::ObservedLeftToRight);
        assert_eq!(first.leading_flags_storage, [7, 6]);
        assert_eq!(first.effect_flags_storage, [5, 4, 0]);
        assert_eq!(first.trailing_flags_storage, [1, 2, 3, 4, 5, 6]);
        assert_eq!(tail.inter_phase_reference_storage, [10; 16]);
        assert_eq!(tail.inter_phase_opaque_storage, [11; 16]);
        Ok(())
    }
    #[test]
    fn independent_phase_blocks_preserve_different_values_and_unknown_codes() -> Result<()> {
        let mut raw = fixture();
        put(&mut raw, 2, &u32::MAX.to_le_bytes());
        put(&mut raw, 69 + 25, &u32::MAX.to_le_bytes());
        put(&mut raw, 176 + 25, &2u32.to_le_bytes());
        let tail = parse_exact(&raw)?;
        assert_eq!(tail.action_mapping, ActionCode::Unknown(u32::MAX));
        assert_eq!(tail.phases.get(1).map(|p| p.growth_mapping), Some(GrowthCode::Unknown(u32::MAX)));
        assert_eq!(tail.phases.get(2).map(|p| p.growth_mapping), Some(GrowthCode::ObservedBottomToTop));
        assert_eq!(tail.phases.iter().map(|p| p.opacity_percent_storage).collect::<Vec<_>>(), vec![60, 61, 62]);
        Ok(())
    }
    #[test]
    fn every_truncation_extra_bytes_and_unknown_framing_are_rejected() {
        let raw = fixture();
        for end in 0..TAIL_BYTES {
            assert!(parse_exact(raw.get(..end).unwrap_or(&[])).is_err());
        }
        let mut extra = raw.clone();
        extra.push(0);
        assert!(parse_exact(&extra).is_err());
        for at in [6, 65, 69, 128, 132, 140, 176, 235, 239] {
            let mut bad = raw.clone();
            put(&mut bad, at, &[0xff]);
            assert!(parse_exact(&bad).is_err());
        }
        let mut nonempty = raw;
        put(&mut nonempty, 136, &1u32.to_le_bytes());
        assert!(parse_exact(&nonempty).is_err());
    }
    #[test]
    fn invalid_scalars_are_rejected_without_normalizing_direction() {
        for at in [12, 16, 20, 75, 79, 83, 182, 186, 190] {
            for v in [f32::NAN, f32::INFINITY, -0.1, 1.1] {
                let mut raw = fixture();
                put(&mut raw, at, &v.to_le_bytes());
                assert!(parse_exact(&raw).is_err());
            }
        }
        for at in [35, 43, 51, 98, 106, 114, 205, 213, 221] {
            let mut raw = fixture();
            put(&mut raw, at, &f64::NAN.to_le_bytes());
            assert!(parse_exact(&raw).is_err());
        }
        let mut raw = fixture();
        put(&mut raw, 24, &101u32.to_le_bytes());
        assert!(parse_exact(&raw).is_err());
    }

    #[test]
    fn interpolation_optionals_shift_later_fields_and_prefix_consumption() -> Result<()> {
        let mut raw = fixture();
        for (flag, finish) in [(200usize, 75u32), (93, 50), (30, 25)] {
            put(&mut raw, flag, &[1]);
            raw.splice(flag + 1..flag + 1, finish.to_le_bytes());
        }
        let tail = parse_exact(&raw)?;
        assert_eq!(tail.byte_length, MAX_TAIL_BYTES);
        assert_eq!(tail.phases.iter().map(|p| p.tail_offset).collect::<Vec<_>>(), vec![6, 73, 184]);
        assert_eq!(tail.phases.iter().map(|p| p.finish_opacity_percent_storage).collect::<Vec<_>>(), vec![Some(25), Some(50), Some(75)]);
        assert_eq!(tail.phases.first().map(|p| p.growth_storage), Some(4));
        assert_eq!(tail.ordinal_storage, 99);
        raw.extend([9, 8]);
        let (_, consumed) = parse_prefix(&raw)?;
        assert_eq!(consumed, MAX_TAIL_BYTES);
        assert!(parse_exact(&raw).is_err());
        Ok(())
    }
    #[test]
    fn unknown_structural_flag_and_invalid_optional_opacity_reject() {
        let mut raw = fixture();
        put(&mut raw, 30, &[2]);
        assert!(parse_exact(&raw).is_err());
        let mut raw = fixture();
        put(&mut raw, 30, &[1]);
        raw.splice(31..31, 101u32.to_le_bytes());
        assert!(parse_exact(&raw).is_err());
    }
}
