use craft_anything_engines::scene_render::{avi::AviWriter, mp4};
use craft_anything_platform::cancel::CancelToken;

#[test]
#[ignore = "requires a local FFmpeg libx264 build via CRAFT_ANYTHING_FFMPEG"]
fn compressed_frames_are_decoded_checked_and_existing_outputs_preserved() {
    let d = tempfile::tempdir().unwrap();
    let input = d.path().join("source.avi");
    let file = std::fs::File::create(&input).unwrap();
    let mut avi = AviWriter::new(file, 64, 48, 6, 7).unwrap();
    for i in 0..7 {
        let mut rgba = Vec::new();
        for y in 0..48 {
            for x in 0..64 {
                rgba.extend([if x < 20 + i * 4 { 220 } else { 20 }, y * 4, 50, 255]);
            }
        }
        avi.frame(&rgba).unwrap();
    }
    drop(avi.finish().unwrap());
    let cancel = CancelToken::default();
    let program = mp4::program(None).unwrap();
    assert!(mp4::probe(&program, &cancel).unwrap().starts_with("ffmpeg version "));
    let output = d.path().join("result.mp4");
    let m = mp4::encode(&program, &input, &output, [64, 48], 7, &cancel).unwrap();
    assert_eq!(m.decoded_rgba_sha256.len(), 7);
    assert_ne!(m.decoded_rgba_sha256[0], m.decoded_rgba_sha256[6]);
    assert!(m.bytes < std::fs::metadata(&input).unwrap().len());
    let bytes = std::fs::read(&output).unwrap();
    assert_eq!(&bytes[4..8], b"ftyp");
    assert!(mp4::encode(&program, &input, &output, [64, 48], 7, &cancel).is_err());
    assert_eq!(std::fs::read(output).unwrap(), bytes);
    assert!(mp4::encode(&program, &input, &d.path().join("wrong-count.mp4"), [64, 48], 6, &cancel).is_err());
    let bad = d.path().join("bad.avi");
    std::fs::write(&bad, b"invalid").unwrap();
    assert!(mp4::encode(&program, &bad, &d.path().join("failed.mp4"), [64, 48], 7, &cancel).is_err());
    cancel.cancel();
    assert!(mp4::encode(&program, &input, &d.path().join("cancel.mp4"), [64, 48], 7, &cancel).is_err());
    assert!(!d.path().join("cancel.mp4").exists());
}
