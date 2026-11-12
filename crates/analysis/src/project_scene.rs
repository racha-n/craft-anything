//! Bounded, headless mapping from declared resource appearance to original-world surfaces.
use crate::project_model::Frame;
use crate::project_simulation::{Phase, PhaseState};
use craft_anything_formats::scene_bundle::{Scene, math::Bounds};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceAppearance {
    pub color: Option<[f32; 3]>,
    pub opacity: Option<f32>,
    /// Existing surface half-space dot(normal, world_position) + d <= 0; no cap geometry.
    pub clip_plane: Option<[f64; 4]>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SceneAppearance {
    pub styles: BTreeMap<usize, InstanceAppearance>,
    pub hidden: BTreeSet<usize>,
    pub ambiguous_instances: usize,
    pub membership_visits: usize,
    pub limitations: Vec<String>,
}
pub fn derive(scene: &Scene, frame: &Frame, mut cancelled: impl FnMut() -> bool) -> Result<SceneAppearance, String> {
    if scene.instances.len() > 100_000 || frame.resource_phases.len() > 10_000 {
        return Err("Appearance scene or resource count exceeds its work budget".into());
    }
    let mut out = SceneAppearance::default();
    let ids = scene.resources.iter().enumerate().map(|(id, r)| (r.guid.as_str(), id)).collect::<BTreeMap<_, _>>();
    let mut effects = BTreeMap::new();
    let mut hidden_resources = BTreeSet::new();
    for phase in &frame.resource_phases {
        if matches!(phase.state, PhaseState::Resolved { phase: Phase::Hidden, .. })
            && let Some(id) = ids.get(phase.resource_guid.as_str())
        {
            hidden_resources.insert(*id);
        }
        if let Some(effect) = phase.appearance_effect.as_ref()
            && let Some(id) = ids.get(phase.resource_guid.as_str())
        {
            if !effect.progress.is_finite()
                || !(0.0..=1.0).contains(&effect.progress)
                || effect.opacity.is_some_and(|n| !n.is_finite() || !(0.0..=1.0).contains(&n))
                || effect.color.is_some_and(|rgb| rgb.iter().any(|n| !n.is_finite() || !(0.0..=1.0).contains(n)))
                || effect.growth_direction.is_some_and(|n| craft_anything_formats::scene_bundle::math::normalized(n).is_err())
            {
                return Err("Invalid resource appearance fields".into());
            }
            if effects.insert(*id, effect).is_some() {
                return Err("Duplicate resource appearance effect".into());
            }
        }
    }
    if effects.is_empty() {
        return Ok(out);
    }
    let visible_without_effect = scene
        .resources
        .iter()
        .enumerate()
        .filter(|(id, _)| !effects.contains_key(id) && !hidden_resources.contains(id))
        .map(|(id, _)| id)
        .collect::<BTreeSet<_>>();
    let mut bounds = BTreeMap::<usize, Bounds>::new();
    for instance in &scene.instances {
        if cancelled() {
            return Err("Scene appearance mapping cancelled".into());
        }
        if !instance.bounds.valid() {
            return Err("Invalid instance bounds during appearance mapping".into());
        }
        for id in &instance.resources {
            out.membership_visits = out.membership_visits.checked_add(1).ok_or("Appearance membership count overflow")?;
            if out.membership_visits > 1_000_000 {
                return Err("Appearance membership visits exceed one million".into());
            }
            if effects.contains_key(id) {
                let b = bounds.entry(*id).or_insert_with(Bounds::empty);
                for point in instance.bounds.0 {
                    b.include(point);
                }
            }
        }
    }
    let mut resource_styles = BTreeMap::new();
    let mut empty_growth = BTreeSet::new();
    for (id, effect) in effects {
        let clip_plane = if let Some(direction) = effect.growth_direction {
            if effect.progress == 0. {
                empty_growth.insert(id);
            }
            let Some(b) = bounds.get(&id) else {
                continue;
            };
            match growth_plane(*b, direction, effect.progress) {
                Ok(plane) => Some(plane),
                Err(_) => {
                    if !out.limitations.iter().any(|s| s == "Some resource growth bounds have no positive projected extent") {
                        out.limitations.push("Some resource growth bounds have no positive projected extent".into());
                    }
                    None
                }
            }
        } else {
            None
        };
        resource_styles.insert(id, InstanceAppearance { color: effect.color, opacity: effect.opacity, clip_plane });
    }
    for (id, instance) in scene.instances.iter().enumerate() {
        if cancelled() {
            return Err("Scene appearance mapping cancelled".into());
        }
        let mut selected = None;
        let mut ambiguous = false;
        for resource in &instance.resources {
            if let Some(style) = resource_styles.get(resource) {
                if selected.is_some_and(|other| other != style) {
                    ambiguous = true;
                    break;
                }
                selected = Some(style);
            }
        }
        if selected.is_some() && instance.resources.iter().any(|id| visible_without_effect.contains(id)) {
            ambiguous = true;
        }
        if ambiguous {
            out.ambiguous_instances += 1;
            out.styles.insert(id, InstanceAppearance { color: None, opacity: None, clip_plane: None });
            continue;
        }
        if let Some(style) = selected {
            if style.opacity == Some(0.) || instance.resources.iter().any(|r| empty_growth.contains(r)) {
                out.hidden.insert(id);
            }
            out.styles.insert(id, style.clone());
        }
    }
    if !out.styles.is_empty() {
        out.limitations.push("Declared resource-world-AABB surface clipping; no new cut caps or native clipping-bound convention is claimed".into());
    }
    if out.ambiguous_instances > 0 {
        out.limitations.push("Conflicting resource effects leave the affected instances without a colour/opacity/clip override".into());
    }
    Ok(out)
}
pub fn growth_plane(bounds: Bounds, direction: [f64; 3], progress: f64) -> Result<[f64; 4], String> {
    if !bounds.valid() || !progress.is_finite() || !(0.0..=1.0).contains(&progress) {
        return Err("Invalid growth bounds or progress".into());
    }
    let direction = craft_anything_formats::scene_bundle::math::normalized(direction).map_err(|_| "Growth direction is zero or nonfinite")?;
    let [low, high] = bounds.0;
    let mut min = 0.;
    let mut max = 0.;
    for ((normal, a), b) in direction.iter().zip(low).zip(high) {
        min += normal * if *normal >= 0. { a } else { b };
        max += normal * if *normal >= 0. { b } else { a };
    }
    let threshold = min + (max - min) * progress;
    if !threshold.is_finite() || max <= min {
        return Err("Growth has a nonfinite or zero projected extent".into());
    }
    let [x, y, z] = direction;
    Ok([x, y, z, -threshold])
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn opposite_directions_keep_opposite_portions_at_the_same_fraction() {
        let b = Bounds([[10., 20., 30.], [14., 26., 38.]]);
        assert_eq!(growth_plane(b, [1., 0., 0.], 0.25).unwrap(), [1., 0., 0., -11.]);
        assert_eq!(growth_plane(b, [-1., 0., 0.], 0.25).unwrap(), [-1., 0., 0., 13.]);
        assert_eq!(growth_plane(b, [0., 0., 1.], 0.5).unwrap(), [0., 0., 1., -34.]);
    }
    #[test]
    fn bad_growth_direction_or_fraction_is_recoverable() {
        let b = Bounds([[0.; 3], [1.; 3]]);
        assert!(growth_plane(b, [0.; 3], 0.5).is_err());
        assert!(growth_plane(b, [f64::NAN, 0., 0.], 0.5).is_err());
        assert!(growth_plane(b, [1., 0., 0.], 1.1).is_err());
    }
}
