//! Explicit preview policies over decoded profile fields. Native project-wide modifiers
//! and mesh clipping/capping fidelity require independent validation.
use craft_anything_formats::{
    project_appearance::{GrowthCode, PhaseBlock},
    project_calendar::WorkAllocation,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgressBasis {
    ElapsedWallclock,
    WorkingCalendar,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppearanceEffect {
    pub progress: f64,
    pub progress_basis: ProgressBasis,
    /// None retains the original material/placeholder colour; it is not black.
    pub color: Option<[f32; 3]>,
    /// None retains original transparency, whose source coverage is separate.
    pub opacity: Option<f32>,
    pub growth_direction: Option<[f64; 3]>,
    pub source_tasks: Vec<String>,
    pub limitations: Vec<String>,
}
pub fn elapsed_progress(start: i64, finish: i64, focus: i64) -> Result<f64, String> {
    let duration = finish.checked_sub(start).filter(|n| *n > 0).ok_or("Invalid appearance interval")?;
    let elapsed = focus.clamp(start, finish).checked_sub(start).ok_or("Appearance elapsed time overflow")?;
    Ok(elapsed as f64 / duration as f64)
}
pub fn working_progress(work: &WorkAllocation, focus: i64) -> Result<f64, String> {
    if work.requested_working_seconds == 0 || work.segments.len() > 65_536 {
        return Err("Appearance working allocation is empty or exceeds its segment budget".into());
    }
    let mut total = 0u64;
    let mut elapsed = 0u64;
    let mut last = None;
    for segment in &work.segments {
        let duration = segment.end_wallclock.checked_sub(segment.start_wallclock).filter(|n| *n > 0).ok_or("Invalid appearance working segment")?;
        let duration = u64::try_from(duration).map_err(|_| "Appearance segment duration overflow")?;
        if duration != segment.working_seconds || last.is_some_and(|end| segment.start_wallclock < end) {
            return Err("Appearance working segments overlap or disagree with their duration".into());
        }
        last = Some(segment.end_wallclock);
        total = total.checked_add(duration).ok_or("Appearance working total overflow")?;
        let span = focus.clamp(segment.start_wallclock, segment.end_wallclock).checked_sub(segment.start_wallclock).ok_or("Appearance progress overflow")?;
        elapsed = elapsed.checked_add(u64::try_from(span).map_err(|_| "Appearance progress is negative")?).ok_or("Appearance progress sum overflow")?;
    }
    if total != work.requested_working_seconds {
        return Err("Appearance working allocation is incomplete".into());
    }
    Ok(elapsed as f64 / total as f64)
}
pub fn active_effect(phase: &PhaseBlock, progress: f64, basis: ProgressBasis) -> Result<AppearanceEffect, String> {
    if !progress.is_finite() || !(0.0..=1.0).contains(&progress) {
        return Err("Appearance progress must be finite and within 0..1".into());
    }
    if phase.rgb_storage.iter().any(|n| !n.is_finite() || !(0.0..=1.0).contains(n))
        || phase.opacity_percent_storage > 100
        || phase.finish_opacity_percent_storage.is_some_and(|n| n > 100)
    {
        return Err("Appearance colour or opacity exceeds its finite range".into());
    }
    if phase.leading_flags_storage.get(1).copied() != Some(1) || phase.effect_flags_storage.get(1).copied() != Some(1) {
        return Err("A non-default phase flag has no admitted visual policy".into());
    }
    if !matches!((phase.effect_flags_storage.get(2).copied(), phase.finish_opacity_percent_storage), (Some(0), None) | (Some(1), Some(_))) {
        return Err("Opacity interpolation flags and payload disagree".into());
    }
    let original_color = *phase.leading_flags_storage.first().ok_or("Original-colour flag missing")?;
    let color = match original_color {
        0 => Some(phase.rgb_storage),
        1 => None,
        _ => return Err("Unknown original-colour flag".into()),
    };
    let original_transparency = *phase.effect_flags_storage.first().ok_or("Original-transparency flag missing")?;
    let mut limitations =
        vec!["Declared progress and surface-clipping preview; native project modifiers, cut caps and intersecting transparency are not established".into()];
    let opacity = match original_transparency {
        0 => {
            let start = f64::from(phase.opacity_percent_storage);
            let finish = f64::from(phase.finish_opacity_percent_storage.unwrap_or(phase.opacity_percent_storage));
            Some(((start + (finish - start) * progress) / 100.0) as f32)
        }
        1 => {
            limitations.push("Original transparency depends on separately admitted source material data".into());
            None
        }
        _ => return Err("Unknown original-transparency flag".into()),
    };
    let growth_direction = match phase.growth_mapping {
        GrowthCode::ObservedNoGrowth => None,
        GrowthCode::ObservedBottomToTop => Some([0., 0., 1.]),
        GrowthCode::ObservedLeftToRight => Some([1., 0., 0.]),
        GrowthCode::Unknown(_) => return Err("Growth direction has no admitted native mapping".into()),
    };
    // The controlled Pause flag alone admits a declared per-assignment calendar preview.
    let mut modifiers = phase.trailing_flags_storage;
    let pause = modifiers.get(2).copied().ok_or("Pause modifier is absent")?;
    if pause > 1 {
        return Err("Unknown Pause modifier".into());
    }
    if let Some(flag) = modifiers.get_mut(2) {
        *flag = 0;
    }
    if pause == 1 && original_transparency == 0 && phase.finish_opacity_percent_storage.is_some_and(|finish| finish != phase.opacity_percent_storage) {
        return Err("Pause combined with opacity interpolation has no independently established clock coupling".into());
    }
    if pause == 1 && basis != ProgressBasis::WorkingCalendar {
        return Err("Pause requires resolved working-calendar progress".into());
    }
    if pause == 1 {
        limitations.push("Pause uses the assigned task calendar; native project-wide modifier precedence remains unverified".into());
    }
    if modifiers != [0, 1, 0, 1, 0, 1] {
        return Err("Non-default growth modifiers need an explicit admitted policy".into());
    }
    Ok(AppearanceEffect { progress, progress_basis: basis, color, opacity, growth_direction, source_tasks: Vec::new(), limitations })
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_anything_formats::project_appearance::PhaseSlot;
    fn phase() -> PhaseBlock {
        PhaseBlock {
            slot: PhaseSlot::First,
            tail_offset: 6,
            byte_length: 67,
            leading_flags_storage: [0, 1],
            rgb_storage: [0.1, 0.2, 0.3],
            opacity_percent_storage: 35,
            finish_opacity_percent_storage: Some(75),
            effect_flags_storage: [0, 1, 1],
            growth_storage: 4,
            growth_mapping: GrowthCode::ObservedLeftToRight,
            custom_direction_storage: [0., 0., 1.],
            trailing_flags_storage: [0, 1, 0, 1, 0, 1],
        }
    }
    #[test]
    fn unknown_phase_flags_and_inconsistent_interpolation_are_unresolved() {
        let mut unknown = phase();
        unknown.leading_flags_storage[1] = 0;
        assert!(active_effect(&unknown, 0.5, ProgressBasis::ElapsedWallclock).is_err());
        let mut unknown = phase();
        unknown.effect_flags_storage[1] = 0;
        assert!(active_effect(&unknown, 0.5, ProgressBasis::ElapsedWallclock).is_err());
        let mut missing = phase();
        missing.finish_opacity_percent_storage = None;
        assert!(active_effect(&missing, 0.5, ProgressBasis::ElapsedWallclock).is_err());
        let mut unexpected = phase();
        unexpected.effect_flags_storage[2] = 0;
        assert!(active_effect(&unexpected, 0.5, ProgressBasis::ElapsedWallclock).is_err());
    }
    #[test]
    fn pause_requires_calendar_progress_and_does_not_advance_in_a_gap() {
        use craft_anything_formats::project_calendar::{WorkAllocation, WorkSegment};
        let work = WorkAllocation {
            calendar_guid: "original-calendar".into(),
            requested_start_wallclock: 0,
            first_work_start_wallclock: Some(0),
            finish_wallclock: 30,
            requested_working_seconds: 20,
            segments: vec![
                WorkSegment { start_wallclock: 0, end_wallclock: 10, working_seconds: 10, day_type_guid: "day".into() },
                WorkSegment { start_wallclock: 20, end_wallclock: 30, working_seconds: 10, day_type_guid: "day".into() },
            ],
            inferred_storage_conventions_used: true,
        };
        assert_eq!(working_progress(&work, 10).unwrap(), 0.5);
        assert_eq!(working_progress(&work, 19).unwrap(), 0.5);
        assert_eq!(working_progress(&work, 25).unwrap(), 0.75);
        let mut paused = phase();
        paused.trailing_flags_storage[2] = 1;
        paused.finish_opacity_percent_storage = None;
        paused.effect_flags_storage[2] = 0;
        assert!(active_effect(&paused, 0.5, ProgressBasis::ElapsedWallclock).is_err());
        let effect = active_effect(&paused, 0.5, ProgressBasis::WorkingCalendar).unwrap();
        assert_eq!(effect.progress_basis, ProgressBasis::WorkingCalendar);
        assert!(effect.limitations.iter().any(|line| line.contains("project-wide")));
        paused.trailing_flags_storage[4] = 1;
        assert!(active_effect(&paused, 0.5, ProgressBasis::WorkingCalendar).is_err());
    }
    #[test]
    fn opacity_interpolates_and_preset_does_not_use_stale_custom_vector() {
        let effect = active_effect(&phase(), 0.25, ProgressBasis::ElapsedWallclock).unwrap();
        assert_eq!(effect.opacity, Some(0.45));
        assert_eq!(effect.growth_direction, Some([1., 0., 0.]));
        assert_eq!(effect.color, Some([0.1, 0.2, 0.3]));
    }
    #[test]
    fn inherited_appearance_is_not_an_explicit_black_or_opaque_override() {
        let mut p = phase();
        p.leading_flags_storage[0] = 1;
        p.effect_flags_storage[0] = 1;
        let effect = active_effect(&p, 0.5, ProgressBasis::ElapsedWallclock).unwrap();
        assert_eq!((effect.color, effect.opacity), (None, None));
    }
    #[test]
    fn unknown_and_nonfinite_effect_inputs_are_rejected() {
        assert!(active_effect(&phase(), f64::NAN, ProgressBasis::ElapsedWallclock).is_err());
        let mut p = phase();
        p.growth_mapping = GrowthCode::Unknown(999);
        assert!(active_effect(&p, 0.5, ProgressBasis::ElapsedWallclock).is_err());
        assert!(elapsed_progress(i64::MIN, i64::MAX, 0).is_err());
    }
    #[test]
    fn elapsed_preview_includes_nonworking_gaps_and_clamps_boundaries() {
        assert_eq!(elapsed_progress(100, 200, 150).unwrap(), 0.5);
        assert_eq!(elapsed_progress(100, 200, 99).unwrap(), 0.);
        assert_eq!(elapsed_progress(100, 200, 201).unwrap(), 1.);
    }
}
