//! Versioned camera/focus paths sampled identically by preview and export.
use super::camera::Camera;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Keyframe {
    pub seconds: f64,
    pub camera: Camera,
    pub focus: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Animation {
    pub version: u32,
    pub scene_id: String,
    pub keyframes: Vec<Keyframe>,
}
impl Animation {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.scene_id.len() != 64 || !self.scene_id.bytes().all(|b| b.is_ascii_hexdigit()) || !(2..=64).contains(&self.keyframes.len())
        {
            return Err("A version-1 camera path needs a scene digest and 2 to 64 keyframes".into());
        }
        let first = self.keyframes.first().ok_or("Missing first keyframe")?;
        if first.seconds != 0. {
            return Err("The first keyframe must start at zero seconds".into());
        }
        let mut previous = -1.;
        for key in &self.keyframes {
            key.camera.validate().map_err(|e| e.to_string())?;
            if !key.seconds.is_finite() || key.seconds <= previous || key.seconds > 600. || key.camera.y_up != first.camera.y_up {
                return Err("Keyframe times must increase within 600 seconds and use the same up axis".into());
            }
            previous = key.seconds;
        }
        Ok(())
    }
    pub fn duration(&self) -> f64 {
        self.keyframes.last().map_or(0., |k| k.seconds)
    }
    pub fn sample(&self, seconds: f64) -> Result<Keyframe, String> {
        self.validate()?;
        if !seconds.is_finite() {
            return Err("Invalid animation time".into());
        }
        let first = self.keyframes.first().ok_or("Missing keyframe")?;
        let last = self.keyframes.last().ok_or("Missing last keyframe")?;
        if seconds <= 0. {
            return Ok(first.clone());
        }
        if seconds >= last.seconds {
            return Ok(last.clone());
        }
        let pair = self
            .keyframes
            .windows(2)
            .find(|p| p.first().zip(p.get(1)).is_some_and(|(a, b)| seconds >= a.seconds && seconds <= b.seconds))
            .ok_or("No enclosing keyframes")?;
        let a = pair.first().ok_or("Missing keyframe")?;
        let b = pair.get(1).ok_or("Missing next keyframe")?;
        let t = (seconds - a.seconds) / (b.seconds - a.seconds);
        let mix = |x: f64, y: f64| x + (y - x) * t;
        let yaw_delta = (b.camera.yaw - a.camera.yaw + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
        let camera = Camera {
            target: std::array::from_fn(|i| mix(a.camera.target[i], b.camera.target[i])),
            yaw: a.camera.yaw + yaw_delta * t,
            pitch: mix(a.camera.pitch, b.camera.pitch),
            distance: (a.camera.distance.ln() + (b.camera.distance.ln() - a.camera.distance.ln()) * t).exp().clamp(1e-6, 1e14),
            y_up: a.camera.y_up,
        };
        Ok(Keyframe { seconds, camera, focus: ((a.focus as f64) + (b.focus as f64 - a.focus as f64) * t).round() as i64 })
    }
    pub fn frame_count(&self, fps: u32) -> Result<u32, String> {
        self.validate()?;
        if !(1..=30).contains(&fps) {
            return Err("Frame rate must be 1 to 30 fps".into());
        }
        let n = (self.duration() * f64::from(fps)).ceil() as u32 + 1;
        if n > 900 {
            return Err("Video exceeds the 900 frame budget; shorten the path or lower fps".into());
        }
        Ok(n)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn path() -> Animation {
        let c = Camera::default();
        let mut d = c.clone();
        d.yaw = c.yaw + std::f64::consts::TAU - 0.2;
        Animation {
            version: 1,
            scene_id: "a".repeat(64),
            keyframes: vec![Keyframe { seconds: 0., camera: c, focus: 100 }, Keyframe { seconds: 2., camera: d, focus: 300 }],
        }
    }
    #[test]
    fn endpoints_short_yaw_and_focus_are_deterministic() {
        let a = path();
        let m = a.sample(1.).unwrap();
        assert!((m.camera.yaw - a.keyframes[0].camera.yaw + 0.1).abs() < 1e-10);
        assert_eq!(m.focus, 200);
        assert_eq!(a.sample(2.).unwrap().camera, a.keyframes[1].camera);
        assert_eq!(a.frame_count(10).unwrap(), 21);
    }
    #[test]
    fn invalid_paths_fail_before_export() {
        let mut a = path();
        a.keyframes[1].seconds = 0.;
        assert!(a.validate().is_err());
        let mut a = path();
        a.keyframes[1].camera.y_up = true;
        assert!(a.validate().is_err());
        assert!(path().sample(f64::NAN).is_err());
        assert!(path().frame_count(0).is_err());
    }
}
