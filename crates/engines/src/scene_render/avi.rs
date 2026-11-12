//! Bounded lossless 24-bit AVI. Frame order and row alignment follow the RIFF/DIB layout.
use std::io::Write;
pub struct AviWriter<W: Write> {
    writer: W,
    width: u32,
    height: u32,
    fps: u32,
    frames: u32,
    written: u32,
    stride: u32,
    frame_bytes: u32,
    pixels: Vec<u8>,
}
fn word(out: &mut Vec<u8>, v: u32) {
    out.extend(v.to_le_bytes());
}
fn chunk(out: &mut Vec<u8>, id: &[u8; 4], data: &[u8]) {
    out.extend(id);
    word(out, data.len() as u32);
    out.extend(data);
}
impl<W: Write> AviWriter<W> {
    pub fn new(mut writer: W, width: u32, height: u32, fps: u32, frames: u32) -> Result<Self, String> {
        if !(2..=1600).contains(&width) || !(2..=1600).contains(&height) || !(1..=30).contains(&fps) || !(2..=900).contains(&frames) {
            return Err("AVI dimensions, fps or frame count exceed export limits".into());
        }
        let stride = (width * 3).div_ceil(4) * 4;
        let frame_bytes = stride * height;
        let total = 232u64 + u64::from(frames) * (u64::from(frame_bytes) + 24);
        if total > 512 * 1024 * 1024 {
            return Err("Uncompressed video exceeds 512 MiB; lower resolution, duration or fps".into());
        }
        let mut avih = Vec::new();
        for v in [1_000_000 / fps, frame_bytes * fps, 0, 0x10, frames, 0, 1, frame_bytes, width, height, 0, 0, 0, 0] {
            word(&mut avih, v);
        }
        let mut strh = Vec::from(*b"vidsDIB ");
        for v in [0, 0, 0, 1, fps, 0, frames, frame_bytes, u32::MAX, 0] {
            word(&mut strh, v);
        }
        strh.extend([0u8; 4]);
        strh.extend((width as u16).to_le_bytes());
        strh.extend((height as u16).to_le_bytes());
        let mut strf = Vec::new();
        for v in [40, width, height, 0x00180001, 0, frame_bytes, 0, 0, 0, 0] {
            word(&mut strf, v);
        }
        let mut strl = Vec::from(*b"strl");
        chunk(&mut strl, b"strh", &strh);
        chunk(&mut strl, b"strf", &strf);
        let mut hdrl = Vec::from(*b"hdrl");
        chunk(&mut hdrl, b"avih", &avih);
        chunk(&mut hdrl, b"LIST", &strl);
        let mut header = Vec::from(*b"RIFF");
        word(&mut header, (total - 8) as u32);
        header.extend(b"AVI ");
        chunk(&mut header, b"LIST", &hdrl);
        header.extend(b"LIST");
        word(&mut header, 4 + frames * (8 + frame_bytes));
        header.extend(b"movi");
        if header.len() != 224 {
            return Err("AVI header size is inconsistent".into());
        }
        writer.write_all(&header).map_err(|e| e.to_string())?;
        Ok(Self { writer, width, height, fps, frames, written: 0, stride, frame_bytes, pixels: Vec::with_capacity(frame_bytes as usize) })
    }
    pub fn frame(&mut self, rgba: &[u8]) -> Result<(), String> {
        if self.written >= self.frames || rgba.len() != self.width as usize * self.height as usize * 4 {
            return Err("AVI frame count or byte length differs from the export plan".into());
        }
        self.pixels.clear();
        for row in rgba.chunks_exact(self.width as usize * 4).rev() {
            for [r, g, b, _] in row.as_chunks::<4>().0 {
                self.pixels.extend([*b, *g, *r]);
            }
            self.pixels.resize(self.pixels.len() + (self.stride - self.width * 3) as usize, 0);
        }
        self.writer
            .write_all(b"00db")
            .and_then(|_| self.writer.write_all(&self.frame_bytes.to_le_bytes()))
            .and_then(|_| self.writer.write_all(&self.pixels))
            .map_err(|e| e.to_string())?;
        self.written += 1;
        Ok(())
    }
    pub fn finish(mut self) -> Result<W, String> {
        if self.written != self.frames {
            return Err("Video is incomplete; not all planned frames were written".into());
        }
        self.writer.write_all(b"idx1").and_then(|_| self.writer.write_all(&(self.frames * 16).to_le_bytes())).map_err(|e| e.to_string())?;
        for i in 0..self.frames {
            let mut entry = Vec::from(*b"00db");
            for v in [0x10, 4 + i * (8 + self.frame_bytes), self.frame_bytes] {
                word(&mut entry, v);
            }
            self.writer.write_all(&entry).map_err(|e| e.to_string())?;
        }
        self.writer.flush().map_err(|e| e.to_string())?;
        Ok(self.writer)
    }
    pub fn encoded_seconds(&self) -> f64 {
        f64::from(self.frames) / f64::from(self.fps)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn riff_lengths_index_and_bottom_up_bgr_are_consistent() {
        let mut w = AviWriter::new(Vec::new(), 2, 2, 10, 2).unwrap();
        let rgba = [255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255];
        w.frame(&rgba).unwrap();
        w.frame(&rgba).unwrap();
        let bytes = w.finish().unwrap();
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize + 8, bytes.len());
        assert_eq!(&bytes[224..228], b"00db");
        assert_eq!(&bytes[232..238], &[255, 0, 0, 255, 0, 0]);
        assert_eq!(&bytes[272..276], b"idx1");
    }
    #[test]
    fn budgets_and_partial_frames_do_not_succeed() {
        assert!(AviWriter::new(Vec::new(), 1600, 1600, 30, 900).is_err());
        let mut w = AviWriter::new(Vec::new(), 2, 2, 10, 2).unwrap();
        assert!(w.frame(&[]).is_err());
        assert!(w.finish().is_err());
    }
}
