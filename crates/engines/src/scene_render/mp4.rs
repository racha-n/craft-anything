//! Optional local H.264 encoding. All child ownership and deadlines belong to platform.
use craft_anything_evidence::ToolError;
use craft_anything_platform::{cancel::CancelToken, process::Process};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path, time::Duration};

const CAP: u64 = 512 * 1024 * 1024;
pub const RECIPE: &str = "h264-crf18-yuv420p-faststart-v1";

fn process(program: &str, cancel: &CancelToken, seconds: u64) -> Process {
    let c = cancel.clone();
    Process::new(program)
        .args(["-nostdin", "-hide_banner"])
        .timeout(Duration::from_secs(seconds))
        .idle_timeout(Duration::from_secs(seconds.min(120)))
        .stdout_cap(64 * 1024)
        .stderr_cap(64 * 1024)
        .cancel_when(move || c.is_cancelled())
        .remediation("Choose a local FFmpeg executable with libx264 support, or use lossless AVI export.")
}
pub fn program(explicit: Option<&str>) -> Result<String, ToolError> {
    let value =
        explicit.filter(|s| !s.trim().is_empty()).map(str::to_owned).or_else(|| std::env::var("CRAFT_ANYTHING_FFMPEG").ok()).unwrap_or_else(|| "ffmpeg".into());
    if value.len() > 4096 || value.trim().is_empty() || value.starts_with('-') || value.contains(['\0', '\r', '\n']) {
        return Err(ToolError::invalid("Choose one FFmpeg executable, without command-line arguments."));
    }
    Ok(value)
}
pub fn probe(program: &str, cancel: &CancelToken) -> Result<String, ToolError> {
    let version = process(program, cancel, 10).arg("-version").run_checked()?.stdout_text();
    let version = version
        .lines()
        .next()
        .filter(|s| s.starts_with("ffmpeg version "))
        .ok_or_else(|| ToolError::invalid("The selected executable did not identify itself as FFmpeg."))?;
    let help = process(program, cancel, 10).args(["-h", "encoder=libx264"]).run_checked()?;
    if !help.stdout_text().contains("Encoder libx264 ") {
        return Err(ToolError::invalid("This FFmpeg build has no libx264 encoder. Choose another build or use AVI."));
    }
    Ok(version.chars().take(512).collect())
}
pub fn validate_size(size: [u32; 2]) -> Result<(), ToolError> {
    if size.iter().any(|n| !(2..=1600).contains(n) || n % 2 != 0) {
        return Err(ToolError::invalid("MP4 requires even width and height within 2..1600. No automatic cropping or resizing is applied."));
    }
    Ok(())
}
#[derive(Debug, Serialize)]
pub struct Encoded {
    pub recipe: &'static str,
    pub bytes: u64,
    pub sha256: String,
    pub decoded_rgba_sha256: Vec<String>,
}
fn hash_file(path: &Path, cancel: &CancelToken) -> Result<(u64, String), ToolError> {
    let mut file = std::fs::File::open(path).map_err(|e| ToolError::io(path.display(), e))?;
    let size = file.metadata().map_err(|e| ToolError::io(path.display(), e))?.len();
    if size == 0 || size > CAP {
        return Err(ToolError::invalid("Encoded MP4 exceeds the output budget or is empty."));
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    let mut total = 0u64;
    loop {
        if cancel.is_cancelled() {
            return Err(ToolError::cancelled());
        }
        let n = file.read(&mut buffer).map_err(|e| ToolError::io(path.display(), e))?;
        if n == 0 {
            break;
        }
        total = total.saturating_add(n as u64);
        if total > CAP {
            return Err(ToolError::invalid("Encoded MP4 grew beyond its output budget."));
        }
        hash.update(buffer.get(..n).ok_or_else(|| ToolError::invalid("Invalid video read size"))?);
    }
    Ok((total, format!("{:x}", hash.finalize())))
}
/// Input and outputs are private paths in an owned staging directory. Publication is the caller's job.
pub fn encode(program: &str, input: &Path, output: &Path, size: [u32; 2], frames: u32, cancel: &CancelToken) -> Result<Encoded, ToolError> {
    validate_size(size)?;
    if !(2..=900).contains(&frames) {
        return Err(ToolError::invalid("MP4 frame count must be within 2..900"));
    }
    if output.exists() {
        return Err(ToolError::invalid("Staged MP4 already exists"));
    }
    let cap = CAP.to_string();
    process(program, cancel, 300)
        .args(["-loglevel", "error", "-n", "-protocol_whitelist", "file", "-i"])
        .arg(input)
        .args([
            "-map",
            "0:v:0",
            "-an",
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "18",
            "-pix_fmt",
            "yuv420p",
            "-threads",
            "2",
            "-movflags",
            "+faststart",
            "-fs",
            &cap,
            "-f",
            "mp4",
        ])
        .arg(output)
        .run_checked()?;
    std::fs::OpenOptions::new().read(true).write(true).open(output).and_then(|f| f.sync_all()).map_err(|e| ToolError::io(output.display(), e))?;
    let (bytes, sha256) = hash_file(output, cancel)?;
    let frame_bytes =
        u64::from(size[0]).checked_mul(u64::from(size[1])).and_then(|n| n.checked_mul(4)).ok_or_else(|| ToolError::invalid("MP4 dimensions overflow"))?;
    let expected = frame_bytes.checked_mul(u64::from(frames)).ok_or_else(|| ToolError::invalid("MP4 decode budget overflow"))?;
    let raw = output.with_extension("decoded.rgba");
    let limit = expected.saturating_add(1).to_string();
    process(program, cancel, 120)
        .args(["-loglevel", "error", "-n", "-protocol_whitelist", "file", "-i"])
        .arg(output)
        .args(["-map", "0:v:0", "-an", "-fps_mode", "passthrough", "-pix_fmt", "rgba", "-threads", "2", "-fs", &limit, "-f", "rawvideo"])
        .arg(&raw)
        .run_checked()?;
    let mut file = std::fs::File::open(&raw).map_err(|e| ToolError::io(raw.display(), e))?;
    if file.metadata().map_err(|e| ToolError::io(raw.display(), e))?.len() != expected {
        return Err(ToolError::invalid("Decoded MP4 frame count or dimensions differ from the rendered path; no video was published."));
    }
    let frame_bytes = usize::try_from(frame_bytes).map_err(|_| ToolError::invalid("MP4 frame too large"))?;
    let mut frame = vec![0; frame_bytes];
    let mut hashes = Vec::new();
    for _ in 0..frames {
        if cancel.is_cancelled() {
            return Err(ToolError::cancelled());
        }
        file.read_exact(&mut frame).map_err(|e| ToolError::io(raw.display(), e))?;
        hashes.push(format!("{:x}", Sha256::digest(&frame)));
    }
    Ok(Encoded { recipe: RECIPE, bytes, sha256, decoded_rgba_sha256: hashes })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn odd_dimensions_and_ambiguous_executables_are_rejected() {
        assert!(validate_size([321, 180]).is_err());
        assert!(validate_size([320, 181]).is_err());
        assert!(validate_size([320, 180]).is_ok());
        assert!(program(Some("-version")).is_err());
        assert!(program(Some("bad\npath")).is_err());
    }
    #[test]
    fn missing_encoder_is_actionable() {
        let error = probe("craft-anything-missing-ffmpeg-test", &CancelToken::default()).unwrap_err();
        assert!(error.for_people().contains("FFmpeg"));
    }
}
