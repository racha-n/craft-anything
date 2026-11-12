//! Pure bounded candidate image decoding; no file access.
use craft_anything_formats::project_textures::ImageCodec;
use image::ImageDecoder;
use std::io::Cursor;
pub(super) const MAX_DECODED_BYTES: usize = 64 * 1024 * 1024;
const MAX_SIDE: u32 = 8192;
pub(super) fn rgba_size(width: u32, height: u32) -> Result<usize, String> {
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return Err("Texture dimensions exceed preview limits".into());
    }
    let size = (width as usize).checked_mul(height as usize).and_then(|n| n.checked_mul(4)).ok_or("Texture size overflow")?;
    if size > MAX_DECODED_BYTES {
        return Err("Texture exceeds decoded preview budget".into());
    }
    Ok(size)
}
pub(super) fn decode_parts(width: u32, height: u32, codec: &ImageCodec, data: &[u8]) -> Result<Vec<u8>, String> {
    let size = rgba_size(width, height)?;
    if matches!(codec, ImageCodec::JpegCarrier) {
        let mut decoder = image::codecs::jpeg::JpegDecoder::new(Cursor::new(data)).map_err(|e| format!("Invalid texture JPEG: {e}"))?;
        if decoder.dimensions() != (width, height) {
            return Err("JPEG dimensions differ from texture header".into());
        }
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(MAX_SIDE);
        limits.max_image_height = Some(MAX_SIDE);
        limits.max_alloc = Some(MAX_DECODED_BYTES as u64);
        decoder.set_limits(limits).map_err(|e| format!("JPEG exceeds decode limits: {e}"))?;
        return image::DynamicImage::from_decoder(decoder).map(|im| im.into_rgba8().into_raw()).map_err(|e| format!("Cannot decode texture JPEG: {e}"));
    }
    let channels = match codec {
        ImageCodec::RawThreeChannelCandidate => 3,
        ImageCodec::RawSingleChannelCandidate => 1,
        ImageCodec::JpegCarrier => return Err("Unexpected JPEG branch".into()),
    };
    if data.len() != size / 4 * channels {
        return Err("Raw texture payload length differs from dimensions".into());
    }
    let mut out = Vec::new();
    out.try_reserve_exact(size).map_err(|_| "Cannot allocate decoded texture")?;
    for p in data.chunks_exact(channels) {
        let r = *p.first().ok_or("Empty texture pixel")?;
        if channels == 1 {
            out.extend_from_slice(&[r, r, r, 255]);
        } else {
            out.extend_from_slice(&[r, *p.get(1).ok_or("Missing green channel")?, *p.get(2).ok_or("Missing blue channel")?, 255]);
        }
    }
    Ok(out)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn raw_candidates_and_lengths() -> Result<(), String> {
        assert_eq!(decode_parts(2, 1, &ImageCodec::RawThreeChannelCandidate, &[255, 0, 0, 0, 255, 0])?, [255, 0, 0, 255, 0, 255, 0, 255]);
        assert_eq!(decode_parts(1, 1, &ImageCodec::RawSingleChannelCandidate, &[37])?, [37, 37, 37, 255]);
        assert!(decode_parts(2, 1, &ImageCodec::RawSingleChannelCandidate, &[37]).is_err());
        Ok(())
    }
    #[test]
    fn original_jpeg_header_dimensions_and_truncation() -> Result<(), String> {
        let mut encoded = Vec::new();
        image::codecs::jpeg::JpegEncoder::new(&mut encoded).encode(&[255, 0, 0, 0, 255, 0], 2, 1, image::ExtendedColorType::Rgb8).map_err(|e| e.to_string())?;
        assert_eq!(decode_parts(2, 1, &ImageCodec::JpegCarrier, &encoded)?.len(), 8);
        assert!(decode_parts(1, 1, &ImageCodec::JpegCarrier, &encoded).is_err());
        assert!(decode_parts(2, 1, &ImageCodec::JpegCarrier, &encoded[..encoded.len() / 2]).is_err());
        Ok(())
    }
    #[test]
    fn dimensions_and_jpeg_reject_before_allocation() {
        for (w, h) in [(0, 1), (9000, 1), (8192, 8192), (u32::MAX, u32::MAX)] {
            assert!(rgba_size(w, h).is_err());
        }
        assert!(decode_parts(1, 1, &ImageCodec::JpegCarrier, &[255, 216, 255]).is_err());
    }
}
