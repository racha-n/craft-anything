//! Single-rate planned budgets and an explicit working-time preview policy.
//! No actual expenditure or remaining-cost semantics are inferred here.

use craft_anything_formats::project_calendar::WorkAllocation;
use craft_anything_formats::project_calendar::{Calendars, DayType};
use std::collections::{BTreeMap, BTreeSet};

use crate::project_rate_policy;

pub const MAX_WORK_SEGMENTS: usize = 65_536;
pub const PLANNED_VALUE_POLICY: &str = "Declared planned-value preview, not native time-phase parity: Fixed accrues at first working start; all other admitted rates spread the final budget linearly over supplied working seconds. Budget formulas have separate, scoped control evidence.";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectiveRate {
    pub effective_wallclock: i64,
    pub type_storage: u32,
    pub amount_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeRateInput {
    pub kind_storage: u8,
    pub unit_storage: Option<u8>,
    pub rates: Vec<EffectiveRate>,
    pub normalized_units_millionths: u64,
    pub nominal_day_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeRatePlan {
    pub budget_minor: i64,
    pub planned_cumulative_minor: Option<i64>,
    pub policy: &'static str,
    pub reasons: Vec<String>,
}

/// Evaluate exactly one time-resource rate effective no later than requested start.
///
/// Callers must explicitly normalize quantities to nominal-day millionths. This
/// function neither selects among rate windows nor derives a nominal calendar day.
/// A focus of `None` returns budget only. Positive, fully verified working spans
/// are required; zero-duration milestones need a separate declared policy.
///
/// Non-Fixed PV is the final rounded budget times completed working fraction,
/// rounded once to the nearest cent (half upward). In particular, Complete-rate
/// PV does not independently round accrued hours; its budget's hour rounding is
/// already included. This is a declared preview, not an observed native PV rule.
pub fn evaluate(input: &TimeRateInput, work: &WorkAllocation, focus: Option<i64>) -> Result<TimeRatePlan, String> {
    if !matches!(input.kind_storage, 1..=3) {
        return Err("Time-rate preview admits only Equipment, Human or Location kind codes 1/2/3".into());
    }
    if input.rates.len() != 1 {
        return Err("Time-rate preview requires exactly one effective rate; resolve multiple or missing rate windows separately".into());
    }
    let rate = input.rates.first().ok_or_else(|| "A single rate is required".to_string())?;
    if rate.effective_wallclock > work.requested_start_wallclock {
        return Err("Rate becomes effective after the requested task start; supply a rate already active at task start".into());
    }
    let (first_start, total_working) = validate_work(work)?;
    let budget_minor = project_rate_policy::budget(
        input.kind_storage,
        input.unit_storage,
        rate.type_storage,
        rate.amount_minor,
        input.normalized_units_millionths,
        input.nominal_day_seconds,
    )?;
    let planned_cumulative_minor = match focus {
        None => None,
        Some(at) if at < first_start => Some(0),
        Some(_) if rate.type_storage == 1 => Some(budget_minor),
        Some(at) => {
            let mut completed = 0_u64;
            for segment in &work.segments {
                if at <= segment.start_wallclock {
                    break;
                }
                let seconds = if at >= segment.end_wallclock {
                    segment.working_seconds
                } else {
                    let span = at.checked_sub(segment.start_wallclock).ok_or_else(arithmetic_error)?;
                    u64::try_from(span).map_err(|_| arithmetic_error())?
                };
                completed = completed.checked_add(seconds).ok_or_else(arithmetic_error)?;
            }
            Some(working_fraction(budget_minor, completed, total_working)?)
        }
    };
    let mut reasons = vec![PLANNED_VALUE_POLICY.to_string(), project_rate_policy::POLICY_EVIDENCE.to_string()];
    if work.inferred_storage_conventions_used {
        reasons.push("The supplied calendar allocation used inferred storage conventions; its provenance remains caller-owned".into());
    }
    Ok(TimeRatePlan { budget_minor, planned_cumulative_minor, policy: PLANNED_VALUE_POLICY, reasons })
}

fn validate_work(work: &WorkAllocation) -> Result<(i64, u64), String> {
    if work.segments.is_empty() || work.requested_working_seconds == 0 {
        return Err("A positive working allocation is required; zero-duration schedules need a separate milestone policy".into());
    }
    if work.segments.len() > MAX_WORK_SEGMENTS {
        return Err(format!("Working allocation exceeds {MAX_WORK_SEGMENTS} segments; reduce the supplied schedule window"));
    }
    let first = work.segments.first().ok_or_else(|| "Working allocation is missing its first segment".to_string())?;
    let last = work.segments.last().ok_or_else(|| "Working allocation is missing its last segment".to_string())?;
    if work.requested_start_wallclock > first.start_wallclock
        || work.first_work_start_wallclock != Some(first.start_wallclock)
        || work.finish_wallclock != last.end_wallclock
    {
        return Err("Working allocation start/first-start/finish fields disagree with its segments; recompute the allocation".into());
    }
    let mut previous_end = None;
    let mut total = 0_u64;
    for segment in &work.segments {
        let span = segment.end_wallclock.checked_sub(segment.start_wallclock).ok_or_else(arithmetic_error)?;
        let seconds = u64::try_from(span).map_err(|_| "Working segments must have positive ordered spans".to_string())?;
        if seconds == 0 || seconds != segment.working_seconds || previous_end.is_some_and(|end| segment.start_wallclock < end) {
            return Err("Working segments must be positive, ordered, nonoverlapping and match their declared working seconds".into());
        }
        total = total.checked_add(seconds).ok_or_else(arithmetic_error)?;
        previous_end = Some(segment.end_wallclock);
    }
    if total != work.requested_working_seconds {
        return Err("Working segment total differs from the requested working duration; supply the complete allocation".into());
    }
    Ok((first.start_wallclock, total))
}

fn working_fraction(amount: i64, completed: u64, total: u64) -> Result<i64, String> {
    let denominator = i128::from(total);
    let value = i128::from(amount)
        .checked_mul(i128::from(completed))
        .and_then(|n| n.checked_add(denominator / 2))
        .and_then(|n| n.checked_div(denominator))
        .ok_or_else(arithmetic_error)?;
    i64::try_from(value).map_err(|_| arithmetic_error())
}

fn arithmetic_error() -> String {
    "Time-rate arithmetic exceeded its checked range; correct the allocation or reduce the budget".into()
}

/// Conservatively infer a uniform working-day length from local referenced types.
/// This is an inference, not a decoded native nominal-day setting. Parents, nil
/// weekdays, mixed day lengths and ambiguous references deliberately return None.
pub fn uniform_day_seconds(calendars: &Calendars, calendar_guid: &str) -> Option<u64> {
    if calendars.records.len() > 10_000 || calendar_guid.is_empty() {
        return None;
    }
    let mut matching = calendars.records.iter().filter(|calendar| calendar.guid == calendar_guid);
    let calendar = matching.next()?;
    if matching.next().is_some() || calendar.parent_guid.is_some() || calendar.day_types.len() > 100_000 || calendar.exceptions.len() > 100_000 {
        return None;
    }
    let mut day_types = BTreeMap::new();
    for day in &calendar.day_types {
        if day.guid.is_empty() || day_types.insert(day.guid.as_str(), day).is_some() {
            return None;
        }
    }
    let mut referenced = BTreeSet::new();
    for weekday in &calendar.weekday_day_type_guids {
        referenced.insert(weekday.as_deref()?);
    }
    let mut exception_dates = BTreeMap::new();
    for exception in &calendar.exceptions {
        if exception_dates.insert(exception.date_wallclock, exception.day_type_guid.as_str()).is_some_and(|previous| previous != exception.day_type_guid) {
            return None;
        }
        referenced.insert(exception.day_type_guid.as_str());
    }
    let mut uniform = None;
    let mut interval_count = 0_usize;
    for guid in referenced {
        let day = day_types.get(guid)?;
        interval_count = interval_count.checked_add(day.intervals.len())?;
        if interval_count > MAX_WORK_SEGMENTS {
            return None;
        }
        if let Some(seconds) = local_working_day_seconds(day)? {
            if uniform.is_some_and(|previous| previous != seconds) {
                return None;
            }
            uniform = Some(seconds);
        }
    }
    uniform
}

fn local_working_day_seconds(day: &DayType) -> Option<Option<u64>> {
    if day.kind_storage == 2 {
        return day.intervals.is_empty().then_some(None);
    }
    if day.kind_storage != 1 || day.intervals.is_empty() || day.intervals.len() > 86_400 {
        return None;
    }
    let mut total = 0_u64;
    let mut previous_end = None;
    for interval in &day.intervals {
        if interval.end_second > 86_400 || previous_end.is_some_and(|end| interval.start_second < end) {
            return None;
        }
        let seconds = interval.end_second.checked_sub(interval.start_second)?;
        if seconds == 0 {
            return None;
        }
        total = total.checked_add(u64::from(seconds))?;
        previous_end = Some(interval.end_second);
    }
    (total <= 86_400).then_some(Some(total))
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_anything_formats::project_calendar::WorkSegment;

    fn work() -> WorkAllocation {
        WorkAllocation {
            calendar_guid: "declared-calendar".into(),
            requested_start_wallclock: -100,
            first_work_start_wallclock: Some(0),
            finish_wallclock: 201_600,
            requested_working_seconds: 86_400,
            segments: [0, 86_400, 172_800]
                .into_iter()
                .map(|start| WorkSegment {
                    start_wallclock: start,
                    end_wallclock: start + 28_800,
                    working_seconds: 28_800,
                    day_type_guid: "declared-day".into(),
                })
                .collect(),
            inferred_storage_conventions_used: false,
        }
    }

    fn input(kind: u8, rate_type: u32, quantity: u64) -> TimeRateInput {
        TimeRateInput {
            kind_storage: kind,
            unit_storage: Some(0),
            rates: vec![EffectiveRate { effective_wallclock: -100, type_storage: rate_type, amount_minor: 1_200 }],
            normalized_units_millionths: quantity,
            nominal_day_seconds: Some(28_800),
        }
    }

    #[test]
    fn scoped_whole_day_budget_predictions_are_preserved() {
        for (kind, rate, expected) in [(2, 1, 1_200), (2, 2, 28_800), (2, 3, 3_600), (1, 3, 3_600), (3, 3, 3_600)] {
            let plan = evaluate(&input(kind, rate, 3_000_000), &work(), None).unwrap();
            assert_eq!(plan.budget_minor, expected);
            assert_eq!(plan.planned_cumulative_minor, None);
            assert!(plan.policy.contains("not native"));
        }
    }

    #[test]
    fn scoped_fractional_hour_predictions_and_declared_pv_are_distinct() {
        for (rate, expected) in [(2, 1_800), (8, 2_400), (3, 225)] {
            let plan = evaluate(&input(2, rate, 187_500), &work(), Some(28_800)).unwrap();
            assert_eq!(plan.budget_minor, expected);
            assert_eq!(plan.planned_cumulative_minor, Some(expected / 3));
        }
        let plan = evaluate(&input(2, 8, 187_500), &work(), Some(3_600)).unwrap();
        assert_eq!(plan.planned_cumulative_minor, Some(100)); // Budget fraction, not an accrued-hour ceiling.
    }

    #[test]
    fn nonworking_gaps_do_not_accrue_and_all_boundaries_are_clamped() {
        let rate = input(1, 3, 3_000_000);
        for (focus, expected) in [(-1, 0), (0, 0), (14_400, 600), (28_800, 1_200), (50_000, 1_200), (86_400, 1_200), (201_600, 3_600), (i64::MAX, 3_600)] {
            assert_eq!(evaluate(&rate, &work(), Some(focus)).unwrap().planned_cumulative_minor, Some(expected));
        }
    }

    #[test]
    fn fixed_preview_steps_at_first_work_not_requested_start() {
        for (focus, expected) in [(-100, 0), (-1, 0), (0, 1_200), (1, 1_200)] {
            assert_eq!(evaluate(&input(2, 1, 0), &work(), Some(focus)).unwrap().planned_cumulative_minor, Some(expected));
        }
    }

    #[test]
    fn missing_or_later_or_multiple_rates_remain_actionably_unresolved() {
        let mut rate = input(2, 3, 1_000_000);
        rate.rates[0].effective_wallclock = -99; // Before first work, but after requested task start.
        assert!(evaluate(&rate, &work(), None).unwrap_err().contains("requested task start"));
        rate.rates.push(rate.rates[0].clone());
        assert!(evaluate(&rate, &work(), None).unwrap_err().contains("exactly one"));
        rate.rates.clear();
        assert!(evaluate(&rate, &work(), None).is_err());
    }

    #[test]
    fn absent_hourly_nominal_and_unknown_basis_are_not_inferred() {
        let mut rate = input(2, 2, 1_000_000);
        rate.nominal_day_seconds = None;
        assert!(evaluate(&rate, &work(), None).unwrap_err().contains("nominal_day_seconds"));
        rate.kind_storage = 4;
        assert!(evaluate(&rate, &work(), None).is_err());
        rate.kind_storage = 2;
        rate.rates[0].type_storage = 9;
        assert!(evaluate(&rate, &work(), None).is_err());
    }

    #[test]
    fn malformed_and_incomplete_allocations_are_rejected() {
        let rate = input(2, 3, 1_000_000);
        for mutation in 0..7 {
            let mut allocation = work();
            match mutation {
                0 => allocation.segments[1].start_wallclock = 1,
                1 => allocation.segments[1].working_seconds -= 1,
                2 => allocation.requested_working_seconds -= 1,
                3 => allocation.finish_wallclock -= 1,
                4 => allocation.first_work_start_wallclock = None,
                5 => allocation.requested_start_wallclock = 1,
                _ => allocation.segments[0].end_wallclock = -1,
            }
            assert!(evaluate(&rate, &allocation, None).is_err());
        }
    }

    #[test]
    fn zero_duration_oversize_and_extreme_spans_are_bounded() {
        let rate = input(2, 3, 1_000_000);
        let mut allocation = work();
        allocation.segments.clear();
        assert!(evaluate(&rate, &allocation, None).unwrap_err().contains("milestone"));
        let mut allocation = work();
        allocation.segments.resize(MAX_WORK_SEGMENTS + 1, allocation.segments[0].clone());
        assert!(evaluate(&rate, &allocation, None).unwrap_err().contains("segments"));
        let mut allocation = work();
        allocation.requested_start_wallclock = i64::MIN;
        allocation.first_work_start_wallclock = Some(i64::MIN);
        allocation.segments[0].start_wallclock = i64::MIN;
        allocation.segments[0].end_wallclock = i64::MAX;
        assert!(evaluate(&rate, &allocation, None).is_err());
    }

    #[test]
    fn supplied_calendar_inference_is_retained_in_reasons() {
        let mut allocation = work();
        allocation.inferred_storage_conventions_used = true;
        let plan = evaluate(&input(2, 3, 1_000_000), &allocation, None).unwrap();
        assert!(plan.reasons.iter().any(|reason| reason.contains("inferred storage")));
    }

    #[test]
    fn final_cent_rounding_is_half_up_without_prorated_unit_rounding() {
        assert_eq!(working_fraction(1, 1, 2), Ok(1));
        assert_eq!(working_fraction(1, 1, 3), Ok(0));
        assert_eq!(working_fraction(1, 2, 3), Ok(1));
        assert_eq!(working_fraction(i64::MAX, u64::MAX, u64::MAX), Ok(i64::MAX));
    }

    fn calendars() -> Calendars {
        use craft_anything_formats::project_calendar::{Calendar, Conventions, DayType, WorkingInterval};
        Calendars {
            schema_version: 1,
            source_bytes: 0,
            raw_header_hex: String::new(),
            root_calendar_guids: vec!["calendar".into()],
            default_calendar_guid: Some("calendar".into()),
            conventions: Conventions::default(),
            records: vec![Calendar {
                offset: 0,
                guid: "calendar".into(),
                name: "Declared calendar".into(),
                parent_guid: None,
                weekday_day_type_guids: std::array::from_fn(|index| Some(if index == 0 || index == 6 { "off" } else { "work" }.into())),
                exceptions: vec![],
                ordinal_storage: 0,
                day_types: vec![
                    DayType {
                        offset: 0,
                        guid: "work".into(),
                        name: "Work".into(),
                        color_storage: [0.0; 3],
                        kind_storage: 1,
                        ordinal_storage: 0,
                        intervals: vec![WorkingInterval { raw_start_wallclock: 0, raw_end_wallclock: 28_800, start_second: 0, end_second: 28_800 }],
                    },
                    DayType {
                        offset: 0,
                        guid: "off".into(),
                        name: "Off".into(),
                        color_storage: [0.0; 3],
                        kind_storage: 2,
                        intervals: vec![],
                        ordinal_storage: 0,
                    },
                ],
            }],
        }
    }

    #[test]
    fn uniform_local_days_allow_nonworking_weekdays_and_exceptions() {
        use craft_anything_formats::project_calendar::DateException;
        let mut data = calendars();
        data.records[0].exceptions.push(DateException { offset: 0, date_wallclock: 100, day_type_guid: "off".into(), ordinal_storage: 0 });
        assert_eq!(uniform_day_seconds(&data, "calendar"), Some(28_800));
        data.records[0].day_types[0].intervals[0].end_second = 36_000;
        assert_eq!(uniform_day_seconds(&data, "calendar"), Some(36_000));
    }

    #[test]
    fn mixed_weekday_or_exception_lengths_are_not_nominal_days() {
        use craft_anything_formats::project_calendar::DateException;
        for exception in [false, true] {
            let mut data = calendars();
            let mut short = data.records[0].day_types[0].clone();
            short.guid = "short".into();
            short.intervals[0].end_second = 25_200;
            data.records[0].day_types.push(short);
            if exception {
                data.records[0].exceptions.push(DateException { offset: 0, date_wallclock: 100, day_type_guid: "short".into(), ordinal_storage: 0 });
            } else {
                data.records[0].weekday_day_type_guids[1] = Some("short".into());
            }
            assert_eq!(uniform_day_seconds(&data, "calendar"), None);
        }
    }

    #[test]
    fn inherited_missing_unknown_and_invalid_types_return_none() {
        for mutation in 0..7 {
            let mut data = calendars();
            match mutation {
                0 => data.records[0].parent_guid = Some("parent".into()),
                1 => data.records[0].weekday_day_type_guids[1] = None,
                2 => data.records[0].weekday_day_type_guids[1] = Some("missing".into()),
                3 => data.records[0].day_types[0].kind_storage = 77,
                4 => data.records[0].day_types[0].intervals[0].end_second = 86_401,
                5 => {
                    let interval = data.records[0].day_types[0].intervals[0].clone();
                    data.records[0].day_types[1].intervals.push(interval);
                }
                _ => data.records[0].weekday_day_type_guids = std::array::from_fn(|_| Some("off".into())),
            }
            assert_eq!(uniform_day_seconds(&data, "calendar"), None);
        }
    }

    #[test]
    fn duplicate_and_conflicting_calendar_references_return_none() {
        use craft_anything_formats::project_calendar::DateException;
        let mut data = calendars();
        data.records.push(data.records[0].clone());
        assert_eq!(uniform_day_seconds(&data, "calendar"), None);
        let mut data = calendars();
        let duplicate = data.records[0].day_types[0].clone();
        data.records[0].day_types.push(duplicate);
        assert_eq!(uniform_day_seconds(&data, "calendar"), None);
        let mut data = calendars();
        for guid in ["off", "work"] {
            data.records[0].exceptions.push(DateException { offset: 0, date_wallclock: 100, day_type_guid: guid.into(), ordinal_storage: 0 });
        }
        assert_eq!(uniform_day_seconds(&data, "calendar"), None);
        assert_eq!(uniform_day_seconds(&calendars(), "missing"), None);
    }
}
