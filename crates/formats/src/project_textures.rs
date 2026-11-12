//! Pure bounded readers for the observed texture-definition and cached-image sections.
//! Paths are provenance strings only and are never opened. Raw channel order, colour space,
//! UV wrapping and V orientation remain unverified. JPEG checks validate framing, not pixels.
use crate::FormatError;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
type Result<T> = std::result::Result<T, FormatError>;
pub const MAX_ASSETS: usize = 1024;
pub const MAX_DIMENSION: u32 = 8192;
pub const MAX_PIXELS: usize = 16 * 1024 * 1024;
pub const MAX_TOTAL_PIXELS: usize = 32 * 1024 * 1024;
pub const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_TOTAL_IMAGE_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_LABEL_BYTES: usize = 4 * 1024 * 1024;
const MAX_IMAGE_SECTION_BYTES: usize = 96 * 1024 * 1024;
const MAX_DEFINITION_BYTES: usize = 16 * 1024 * 1024;
const MAX_JPEG_MARKERS: usize = 16_384;
const MAX_JPEG_SCANS: usize = 256;
const OPEN: [u8; 4] = [0x7a, 0x54, 0xaf, 0x39];
const CLOSE: [u8; 4] = [0x16, 0xe4, 0x99, 0x6f];
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageCodec {
    RawThreeChannelCandidate,
    RawSingleChannelCandidate,
    JpegCarrier,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JpegInfo {
    pub width: u32,
    pub height: u32,
    pub precision_bits: u8,
    pub components: u8,
    pub sof_marker: u8,
    pub scans: usize,
    pub markers: usize,
    pub consumed_bytes: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct TextureDefinition {
    pub guid: String,
    pub label: String,
    pub description: String,
    pub image_guid: String,
    pub source_path_storage: String,
    pub ordinal_storage: u32,
    pub section_offset: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct CachedImage {
    pub guid: String,
    pub label: String,
    pub width: u32,
    pub height: u32,
    pub codec_storage: u32,
    pub codec: ImageCodec,
    pub data_flag_storage: u8,
    pub source_path_storage: String,
    pub ordinal_storage: u32,
    pub section_offset: usize,
    pub payload_offset: usize,
    pub payload_bytes: usize,
    pub sha256: String,
    pub jpeg: Option<JpegInfo>,
    /// Encoded bytes remain available in memory; metadata serialization omits this payload.
    #[serde(skip_serializing)]
    pub data: Vec<u8>,
}
#[derive(Debug, Clone, Serialize)]
pub struct TextureAssets {
    pub schema_version: u32,
    pub textures: Vec<TextureDefinition>,
    pub images: Vec<CachedImage>,
    /// Definition GUID -> validated cached-image index; ordering is not a native priority rule.
    pub texture_image_indices: BTreeMap<String, usize>,
    pub total_payload_bytes: usize,
    pub total_pixels: usize,
    pub pixels_decoded: bool,
}
impl TextureAssets {
    pub fn image_for_texture(&self, texture_guid: &str) -> Option<&CachedImage> {
        self.texture_image_indices.get(texture_guid).and_then(|index| self.images.get(*index))
    }
}
fn malformed(message: &str) -> FormatError {
    FormatError::Malformed(message.into())
}
fn unsupported(message: &str) -> FormatError {
    FormatError::Unsupported(message.into())
}
fn over(message: &str) -> FormatError {
    FormatError::TooLarge(message.into())
}
fn plus(a: usize, b: usize) -> Result<usize> {
    a.checked_add(b).ok_or_else(|| over("texture offset arithmetic"))
}
fn range(raw: &[u8], at: usize, size: usize) -> Result<&[u8]> {
    raw.get(at..plus(at, size)?).ok_or_else(|| FormatError::Truncated("texture field".into()))
}
fn check(callback: &mut dyn FnMut(f32) -> bool, progress: f32) -> Result<()> {
    if callback(progress) { Ok(()) } else { Err(unsupported("texture parsing cancelled")) }
}
struct Reader<'a> {
    raw: &'a [u8],
    pos: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let bytes = range(self.raw, self.pos, n)?;
        self.pos = plus(self.pos, n)?;
        Ok(bytes)
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        self.take(N)?.try_into().map_err(|_| malformed("texture fixed field"))
    }
    fn integer(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.array()?))
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(u8::from_le_bytes(self.array()?))
    }
    fn marker(&mut self, expected: &[u8]) -> Result<()> {
        if self.take(expected.len())? == expected { Ok(()) } else { Err(unsupported("texture record framing")) }
    }
    fn zero(&mut self, n: usize) -> Result<()> {
        if self.take(n)?.iter().all(|v| *v == 0) { Ok(()) } else { Err(unsupported("unobserved texture reserved fields")) }
    }
    fn guid(&mut self) -> Result<String> {
        let raw = self.take(16)?;
        if raw.iter().all(|v| *v == 0) {
            return Err(malformed("nil texture identity/reference"));
        }
        let first = u32::from_le_bytes(range(raw, 0, 4)?.try_into().map_err(|_| malformed("texture GUID"))?);
        let second = u16::from_le_bytes(range(raw, 4, 2)?.try_into().map_err(|_| malformed("texture GUID"))?);
        let third = u16::from_le_bytes(range(raw, 6, 2)?.try_into().map_err(|_| malformed("texture GUID"))?);
        let suffix = range(raw, 8, 8)?.iter().map(|b| format!("{b:02x}")).collect::<String>();
        Ok(format!(
            "{first:08x}-{second:04x}-{third:04x}-{}-{}",
            suffix.get(..4).ok_or_else(|| malformed("texture GUID suffix"))?,
            suffix.get(4..).ok_or_else(|| malformed("texture GUID suffix"))?
        ))
    }
    fn text(&mut self, text_bytes: &mut usize) -> Result<String> {
        self.marker(&[255, 254, 255])?;
        let count = self.byte()?;
        let count = if count == 255 { usize::from(u16::from_le_bytes(self.array()?)) } else { usize::from(count) };
        if count > 16_384 {
            return Err(over("texture string units"));
        }
        let bytes = self.take(count.checked_mul(2).ok_or_else(|| over("texture string arithmetic"))?)?;
        let units = bytes.as_chunks::<2>().0.iter().copied().map(u16::from_le_bytes).collect::<Vec<_>>();
        let text = String::from_utf16(&units).map_err(|_| malformed("texture UTF16"))?;
        *text_bytes = plus(*text_bytes, text.len())?;
        if *text_bytes > MAX_LABEL_BYTES {
            return Err(over("texture metadata text budget"));
        }
        Ok(text)
    }
    fn start(raw: &'a [u8]) -> Result<(Self, usize)> {
        let mut reader = Self { raw, pos: 0 };
        reader.zero(8)?;
        reader.take(16)?;
        reader.marker(&OPEN)?;
        let count = usize::try_from(reader.integer()?).map_err(|_| over("texture asset count"))?;
        if count > MAX_ASSETS {
            return Err(over("texture asset count"));
        }
        Ok((reader, count))
    }
    fn finish(mut self) -> Result<()> {
        self.marker(&CLOSE)?;
        if self.pos != self.raw.len() {
            return Err(unsupported("trailing texture section bytes"));
        }
        Ok(())
    }
}
fn pixels(width: u32, height: u32) -> Result<usize> {
    if width == 0 || height == 0 {
        return Err(malformed("zero cached-image dimensions"));
    }
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(over("cached-image dimension limit"));
    }
    let n = usize::try_from(width)
        .map_err(|_| over("image width"))?
        .checked_mul(usize::try_from(height).map_err(|_| over("image height"))?)
        .ok_or_else(|| over("image pixel arithmetic"))?;
    if n > MAX_PIXELS {
        return Err(over("cached-image pixel budget"));
    }
    Ok(n)
}
fn jpeg(raw: &[u8], width: u32, height: u32, callback: &mut dyn FnMut(f32) -> bool) -> Result<JpegInfo> {
    if range(raw, 0, 2)? != [255, 216] {
        return Err(malformed("cached JPEG SOI"));
    }
    let mut pos = 2usize;
    let mut entropy = false;
    let mut markers = 0usize;
    let mut scans = 0usize;
    let mut frame = None;
    loop {
        check(callback, 0.5)?;
        if entropy {
            loop {
                if pos.is_multiple_of(4096) {
                    check(callback, 0.5)?;
                }
                let byte = *raw.get(pos).ok_or_else(|| FormatError::Truncated("JPEG entropy data".into()))?;
                if byte == 255 {
                    break;
                }
                pos = plus(pos, 1)?;
            }
        } else if raw.get(pos).copied() != Some(255) {
            return Err(malformed("JPEG marker prefix"));
        }
        while raw.get(pos).copied() == Some(255) {
            if pos.is_multiple_of(4096) {
                check(callback, 0.5)?;
            }
            pos = plus(pos, 1)?;
        }
        let marker = *raw.get(pos).ok_or_else(|| FormatError::Truncated("JPEG marker".into()))?;
        pos = plus(pos, 1)?;
        if entropy && (marker == 0 || (0xd0..=0xd7).contains(&marker)) {
            continue;
        }
        entropy = false;
        markers = plus(markers, 1)?;
        if markers > MAX_JPEG_MARKERS {
            return Err(over("JPEG marker budget"));
        }
        if marker == 0xd9 {
            let (precision_bits, components, sof_marker) = frame.ok_or_else(|| malformed("JPEG lacks a frame header"))?;
            if scans == 0 || pos != raw.len() {
                return Err(unsupported("cached JPEG must contain one complete stream without trailing data"));
            }
            return Ok(JpegInfo { width, height, precision_bits, components, sof_marker, scans, markers, consumed_bytes: pos });
        }
        if marker == 0xd8 || marker == 0 || (0xd0..=0xd7).contains(&marker) {
            return Err(malformed("unexpected JPEG marker"));
        }
        if marker == 1 {
            continue;
        }
        let length = usize::from(u16::from_be_bytes(range(raw, pos, 2)?.try_into().map_err(|_| malformed("JPEG segment length"))?));
        if length < 2 {
            return Err(malformed("JPEG segment length below two"));
        }
        let end = plus(pos, length)?;
        let segment = range(raw, plus(pos, 2)?, length - 2)?;
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            if frame.is_some() {
                return Err(unsupported("multiple JPEG frame headers"));
            }
            let precision = *segment.first().ok_or_else(|| malformed("JPEG frame precision"))?;
            let actual_height = u32::from(u16::from_be_bytes(range(segment, 1, 2)?.try_into().map_err(|_| malformed("JPEG height"))?));
            let actual_width = u32::from(u16::from_be_bytes(range(segment, 3, 2)?.try_into().map_err(|_| malformed("JPEG width"))?));
            let components = *segment.get(5).ok_or_else(|| malformed("JPEG component count"))?;
            if precision != 8 {
                return Err(unsupported("cached JPEG precision is not eight bits"));
            }
            if !(1..=4).contains(&components) || segment.len() != 6 + usize::from(components) * 3 {
                return Err(malformed("JPEG frame component framing"));
            }
            if actual_width != width || actual_height != height {
                return Err(malformed("cached JPEG dimensions disagree with image header"));
            }
            frame = Some((precision, components, marker));
        }
        if marker == 0xda {
            if frame.is_none() {
                return Err(malformed("JPEG scan precedes frame"));
            }
            let components = *segment.first().ok_or_else(|| malformed("JPEG scan component count"))?;
            if !(1..=4).contains(&components) || segment.len() != 4 + usize::from(components) * 2 {
                return Err(malformed("JPEG scan framing"));
            }
            scans = plus(scans, 1)?;
            if scans > MAX_JPEG_SCANS {
                return Err(over("JPEG scan budget"));
            }
            entropy = true;
        }
        pos = end;
    }
}
/// Decode both complete retained sections. `true` continues and `false` cancels.
/// JPEG carriers are structurally checked but no image pixels or external paths are decoded.
pub fn parse(definitions: &[u8], graphical_images: &[u8], mut keep_going: impl FnMut(f32) -> bool) -> Result<TextureAssets> {
    let mut progress = 0f32;
    parse_inner(definitions, graphical_images, &mut |p| {
        progress = progress.max(p);
        keep_going(progress)
    })
}
fn parse_inner(definitions: &[u8], graphical_images: &[u8], callback: &mut dyn FnMut(f32) -> bool) -> Result<TextureAssets> {
    check(callback, 0.)?;
    if definitions.len() > MAX_DEFINITION_BYTES || graphical_images.len() > MAX_IMAGE_SECTION_BYTES {
        return Err(over("texture section byte budget"));
    }
    let mut text_bytes = 0usize;
    let (mut reader, count) = Reader::start(definitions)?;
    let mut textures = Vec::new();
    let mut seen = BTreeSet::new();
    textures.try_reserve(count).map_err(|_| over("texture definition allocation"))?;
    for _ in 0..count {
        check(callback, 0.1)?;
        let offset = reader.pos;
        reader.marker(&[0x5d, 0xd2, 0x42, 0x28])?;
        let guid = reader.guid()?;
        if !seen.insert(guid.clone()) {
            return Err(malformed("duplicate texture definition GUID"));
        }
        let label = reader.text(&mut text_bytes)?;
        let description = reader.text(&mut text_bytes)?;
        reader.zero(16)?;
        reader.marker(&[0x59, 0x1e, 0x9d, 0x30])?;
        let image_guid = reader.guid()?;
        let path = reader.text(&mut text_bytes)?;
        let ordinal = reader.integer()?;
        textures.push(TextureDefinition { guid, label, description, image_guid, source_path_storage: path, ordinal_storage: ordinal, section_offset: offset });
    }
    reader.finish()?;
    let (mut reader, count) = Reader::start(graphical_images)?;
    let mut images = Vec::new();
    let mut image_indices = BTreeMap::new();
    let mut total_bytes = 0usize;
    let mut total_pixels = 0usize;
    images.try_reserve(count).map_err(|_| over("cached-image allocation"))?;
    for index in 0..count {
        check(callback, 0.2)?;
        let offset = reader.pos;
        reader.marker(&[0x11, 0xd6, 0x88, 0x5c])?;
        let guid = reader.guid()?;
        if image_indices.insert(guid.clone(), index).is_some() {
            return Err(malformed("duplicate cached-image GUID"));
        }
        let label = reader.text(&mut text_bytes)?;
        reader.zero(16)?;
        let width = reader.integer()?;
        let height = reader.integer()?;
        let n = pixels(width, height)?;
        total_pixels = plus(total_pixels, n)?;
        if total_pixels > MAX_TOTAL_PIXELS {
            return Err(over("cached-image total pixel budget"));
        }
        let codec_storage = reader.integer()?;
        let codec = match codec_storage {
            3 => ImageCodec::RawThreeChannelCandidate,
            5 => ImageCodec::RawSingleChannelCandidate,
            6 => ImageCodec::JpegCarrier,
            _ => return Err(unsupported("unknown cached-image codec")),
        };
        let data_flag = reader.byte()?;
        if data_flag != 1 {
            return Err(unsupported("unknown cached-image payload flag"));
        }
        let bytes = usize::try_from(reader.integer()?).map_err(|_| over("cached-image payload size"))?;
        if bytes > MAX_IMAGE_BYTES {
            return Err(over("cached-image payload budget"));
        }
        total_bytes = plus(total_bytes, bytes)?;
        if total_bytes > MAX_TOTAL_IMAGE_BYTES {
            return Err(over("cached-image total payload budget"));
        }
        match codec {
            ImageCodec::RawThreeChannelCandidate if bytes != n.checked_mul(3).ok_or_else(|| over("raw image arithmetic"))? => {
                return Err(malformed("raw three-channel image length"));
            }
            ImageCodec::RawSingleChannelCandidate if bytes != n => return Err(malformed("raw single-channel image length")),
            _ => {}
        }
        let payload_offset = reader.pos;
        let payload = reader.take(bytes)?;
        let jpeg = if codec == ImageCodec::JpegCarrier { Some(jpeg(payload, width, height, callback)?) } else { None };
        let mut data = Vec::new();
        data.try_reserve(bytes).map_err(|_| over("cached-image byte allocation"))?;
        let mut hash = Sha256::new();
        for chunk in payload.chunks(65_536) {
            check(callback, 0.7)?;
            data.extend_from_slice(chunk);
            hash.update(chunk);
        }
        let source_path_storage = reader.text(&mut text_bytes)?;
        reader.marker(&[0x8d, 0x98, 0xa3, 0xed])?;
        let ordinal_storage = reader.integer()?;
        images.push(CachedImage {
            guid,
            label,
            width,
            height,
            codec_storage,
            codec,
            data_flag_storage: data_flag,
            source_path_storage,
            ordinal_storage,
            section_offset: offset,
            payload_offset,
            payload_bytes: bytes,
            sha256: format!("{:x}", hash.finalize()),
            jpeg,
            data,
        });
    }
    reader.finish()?;
    let mut texture_image_indices = BTreeMap::new();
    for texture in &textures {
        check(callback, 0.9)?;
        let index = image_indices.get(&texture.image_guid).copied().ok_or_else(|| malformed("texture references a missing cached image"))?;
        texture_image_indices.insert(texture.guid.clone(), index);
    }
    check(callback, 1.)?;
    Ok(TextureAssets { schema_version: 1, textures, images, texture_image_indices, total_payload_bytes: total_bytes, total_pixels, pixels_decoded: false })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn text(raw: &mut Vec<u8>, value: &str) {
        let units = value.encode_utf16().collect::<Vec<_>>();
        raw.extend([255, 254, 255, units.len() as u8]);
        for unit in units {
            raw.extend(unit.to_le_bytes());
        }
    }
    fn section(count: u32) -> Vec<u8> {
        let mut raw = vec![0; 24];
        raw.extend(OPEN);
        raw.extend(count.to_le_bytes());
        raw
    }
    fn tiny_jpeg() -> Vec<u8> {
        let mut raw = vec![255, 216, 255, 192, 0, 17, 8, 0, 2, 0, 2, 3, 1, 17, 0, 2, 17, 0, 3, 17, 0];
        raw.extend([255, 218, 0, 12, 3, 1, 0, 2, 0, 3, 0, 0, 63, 0]);
        raw.extend([1, 2, 255, 0, 3, 255, 208, 4, 255, 217]);
        raw
    }
    fn fixture(codec: u32, payload: &[u8]) -> (Vec<u8>, Vec<u8>, usize) {
        let mut defs = section(1);
        defs.extend([0x5d, 0xd2, 0x42, 0x28]);
        defs.extend([1; 16]);
        text(&mut defs, "Texture");
        text(&mut defs, "source label");
        defs.extend([0; 16]);
        defs.extend([0x59, 0x1e, 0x9d, 0x30]);
        defs.extend([2; 16]);
        text(&mut defs, "owned/source.png");
        defs.extend(7u32.to_le_bytes());
        defs.extend(CLOSE);
        let mut imgs = section(1);
        imgs.extend([0x11, 0xd6, 0x88, 0x5c]);
        imgs.extend([2; 16]);
        text(&mut imgs, "Image");
        let header = imgs.len();
        imgs.extend([0; 16]);
        imgs.extend(2u32.to_le_bytes());
        imgs.extend(2u32.to_le_bytes());
        imgs.extend(codec.to_le_bytes());
        imgs.push(1);
        imgs.extend((payload.len() as u32).to_le_bytes());
        imgs.extend(payload);
        text(&mut imgs, "owned/source.png");
        imgs.extend([0x8d, 0x98, 0xa3, 0xed]);
        imgs.extend(4u32.to_le_bytes());
        imgs.extend(CLOSE);
        (defs, imgs, header)
    }
    fn put(raw: &mut [u8], at: usize, value: &[u8]) {
        if let Some(dest) = raw.get_mut(at..at + value.len()) {
            dest.copy_from_slice(value);
        }
    }
    #[test]
    fn raw_payloads_and_guid_references_are_retained_without_channel_claims() -> Result<()> {
        for (code, data, codec) in [(3, vec![1; 12], ImageCodec::RawThreeChannelCandidate), (5, vec![2; 4], ImageCodec::RawSingleChannelCandidate)] {
            let (defs, imgs, _) = fixture(code, &data);
            let parsed = parse(&defs, &imgs, |_| true)?;
            assert_eq!(parsed.images.len(), 1);
            assert_eq!(parsed.textures.len(), 1);
            assert!(!parsed.pixels_decoded);
            assert_eq!(parsed.total_pixels, 4);
            let texture = parsed.textures.first().ok_or_else(|| malformed("test texture"))?;
            let image = parsed.image_for_texture(&texture.guid).ok_or_else(|| malformed("test image"))?;
            assert_eq!(image.codec, codec);
            assert_eq!(image.data, data);
            assert_eq!(image.width, 2);
            assert_eq!(image.height, 2);
            assert!(image.jpeg.is_none());
            let metadata = serde_json::to_value(&parsed).map_err(|_| malformed("test metadata"))?;
            assert!(metadata.get("images").and_then(|v| v.as_array()).and_then(|v| v.first()).and_then(|v| v.get("data")).is_none());
        }
        Ok(())
    }
    #[test]
    fn jpeg_one_stream_reports_actual_dimensions_components_and_scan_count() -> Result<()> {
        let (defs, imgs, _) = fixture(6, &tiny_jpeg());
        let assets = parse(&defs, &imgs, |_| true)?;
        let image = assets.images.first().ok_or_else(|| malformed("test image"))?;
        let header = image.jpeg.as_ref().ok_or_else(|| malformed("test JPEG"))?;
        assert_eq!((header.width, header.height, header.components, header.precision_bits, header.scans), (2, 2, 3, 8, 1));
        assert_eq!(header.consumed_bytes, tiny_jpeg().len());
        let mut extra = tiny_jpeg();
        extra.extend(tiny_jpeg());
        let (defs, imgs, _) = fixture(6, &extra);
        assert!(parse(&defs, &imgs, |_| true).is_err());
        Ok(())
    }
    #[test]
    fn truncations_bad_framing_utf16_and_missing_refs_reject() {
        let (defs, imgs, _) = fixture(3, &[0; 12]);
        for n in 0..defs.len() {
            assert!(parse(defs.get(..n).unwrap_or(&[]), &imgs, |_| true).is_err());
        }
        for n in 0..imgs.len() {
            assert!(parse(&defs, imgs.get(..n).unwrap_or(&[]), |_| true).is_err());
        }
        let mut bad = defs.clone();
        put(&mut bad, 32, &[0]);
        assert!(parse(&bad, &imgs, |_| true).is_err());
        let mut bad = defs.clone();
        put(&mut bad, 56, &0xd800u16.to_le_bytes());
        assert!(parse(&bad, &imgs, |_| true).is_err());
        let mut bad = imgs.clone();
        put(&mut bad, 36, &[3; 16]);
        assert!(parse(&defs, &bad, |_| true).is_err());
        let mut bad = imgs.clone();
        bad.push(0);
        assert!(parse(&defs, &bad, |_| true).is_err());
        let mut bad = defs;
        put(&mut bad, 36, &[0; 16]);
        assert!(parse(&bad, &imgs, |_| true).is_err());
    }
    #[test]
    fn duplicates_counts_dimensions_and_payload_sizes_are_bounded() {
        let (defs, imgs, header) = fixture(3, &[0; 12]);
        let mut bad = defs.clone();
        put(&mut bad, 28, &((MAX_ASSETS + 1) as u32).to_le_bytes());
        assert!(matches!(parse(&bad, &imgs, |_| true), Err(FormatError::TooLarge(_))));
        let mut bad = imgs.clone();
        put(&mut bad, header + 16, &(MAX_DIMENSION + 1).to_le_bytes());
        assert!(matches!(parse(&defs, &bad, |_| true), Err(FormatError::TooLarge(_))));
        let mut bad = imgs.clone();
        put(&mut bad, header + 29, &((MAX_IMAGE_BYTES + 1) as u32).to_le_bytes());
        assert!(matches!(parse(&defs, &bad, |_| true), Err(FormatError::TooLarge(_))));
        for width in [0u32, 1] {
            let mut bad = imgs.clone();
            put(&mut bad, header + 16, &width.to_le_bytes());
            assert!(parse(&defs, &bad, |_| true).is_err());
        }
        let mut bad = imgs.clone();
        let record = imgs.get(32..imgs.len() - 4).unwrap_or(&[]);
        put(&mut bad, 28, &2u32.to_le_bytes());
        bad.truncate(bad.len() - 4);
        bad.extend(record);
        bad.extend(CLOSE);
        assert!(parse(&defs, &bad, |_| true).is_err());
    }
    #[test]
    fn unknown_codecs_flags_and_nonzero_reserved_nan_pattern_are_unsupported() {
        let (defs, imgs, header) = fixture(3, &[0; 12]);
        for (code, flag) in [(99u32, 1u8), (3, 2)] {
            let mut bad = imgs.clone();
            put(&mut bad, header + 24, &code.to_le_bytes());
            put(&mut bad, header + 28, &[flag]);
            assert!(matches!(parse(&defs, &bad, |_| true), Err(FormatError::Unsupported(_))));
        }
        // These frames have no floating-point fields. Do not interpret a reserved byte window
        // as numbers, and do not forbid arbitrary bit patterns inside actual pixel payloads.
        let mut bad = imgs;
        put(&mut bad, header, &f32::NAN.to_le_bytes());
        assert!(matches!(parse(&defs, &bad, |_| true), Err(FormatError::Unsupported(_))));
    }
    #[test]
    fn cancellation_and_empty_sections_are_honest() -> Result<()> {
        let (defs, imgs, _) = fixture(6, &tiny_jpeg());
        assert!(parse(&defs, &imgs, |_| false).is_err());
        let mut calls = 0;
        assert!(
            parse(&defs, &imgs, |_| {
                calls += 1;
                calls < 4
            })
            .is_err()
        );
        let mut empty = section(0);
        empty.extend(CLOSE);
        let assets = parse(&empty, &empty, |_| true)?;
        assert!(assets.images.is_empty());
        assert!(assets.textures.is_empty());
        Ok(())
    }
    #[test]
    fn malformed_jpeg_markers_and_header_mismatch_reject() {
        let raw = tiny_jpeg();
        for n in 0..raw.len() {
            let (defs, imgs, _) = fixture(6, raw.get(..n).unwrap_or(&[]));
            assert!(parse(&defs, &imgs, |_| true).is_err());
        }
        let (defs, mut imgs, header) = fixture(6, &raw);
        put(&mut imgs, header + 16, &3u32.to_le_bytes());
        assert!(parse(&defs, &imgs, |_| true).is_err());
        let mut corrupt = raw;
        put(&mut corrupt, 4, &[0, 1]);
        let (defs, imgs, _) = fixture(6, &corrupt);
        assert!(parse(&defs, &imgs, |_| true).is_err());
    }
}
