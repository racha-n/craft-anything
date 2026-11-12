//! Bounded sampling of the same evaluator used by individual timeline frames.
use crate::project_model::{AppearancePolicy, DateMode, ProjectModel};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sample {
    pub focus_wallclock: i64,
    pub direct_budget_minor: i64,
    pub resource_forecast_minor: i64,
    pub planned_value_minor: i64,
    pub recorded_expenses_minor: Option<i64>,
    pub partial_recorded_expenses_minor: Option<i64>,
    pub unresolved_budget_lines: usize,
    pub unresolved_direct_budget_lines: usize,
    pub unresolved_resource_budget_lines: usize,
    pub unresolved_planned_lines: usize,
    pub actual_partial_minor: i64,
    pub remaining_partial_minor: i64,
    pub unresolved_actual_lines: usize,
    pub unresolved_remaining_lines: usize,
    pub unknown_expense_collections: Option<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Curve {
    pub version: u32,
    pub samples: Vec<Sample>,
    pub date_mode: DateMode,
    pub native_parity_verified: bool,
}
/// Inclusive endpoints. No timezone/unit/currency conversion or interpolation of missing values.
pub fn sample(model: &ProjectModel, start: i64, end: i64, count: usize, date_mode: DateMode, mut cancelled: impl FnMut() -> bool) -> Result<Curve, String> {
    if !(2..=512).contains(&count) || end <= start {
        return Err("Choose 2..512 samples and an end after the start".into());
    }
    if model.tasks.len().checked_mul(count).is_none_or(|n| n > 250_000) {
        return Err("Curve task/sample budget exceeds 250,000; reduce the sample count".into());
    }
    let width = i128::from(end) - i128::from(start);
    let mut samples = Vec::new();
    for i in 0..count {
        if cancelled() {
            return Err("Cost curve sampling cancelled".into());
        }
        let time = i128::from(start) + width * i as i128 / (count - 1) as i128;
        let focus = i64::try_from(time).map_err(|_| "Curve time overflow")?;
        let frame = model.evaluate_with_policy(focus, date_mode, AppearancePolicy::Strict, &mut cancelled)?;
        let costs = frame.costs;
        let expenses = costs.recorded_actual_expenses.as_ref();
        samples.push(Sample {
            focus_wallclock: focus,
            direct_budget_minor: costs.direct_budget_minor,
            resource_forecast_minor: costs.resource_budget_minor,
            planned_value_minor: costs.planned_cumulative_minor,
            recorded_expenses_minor: expenses.and_then(|e| e.recorded_at_focus_minor),
            partial_recorded_expenses_minor: expenses.map(|e| e.partial_recorded_at_focus_minor),
            unresolved_budget_lines: costs.unresolved_budget_lines,
            unresolved_direct_budget_lines: costs.unresolved_direct_budget_lines,
            unresolved_resource_budget_lines: costs.unresolved_resource_budget_lines,
            unresolved_planned_lines: costs.unresolved_cumulative_lines,
            actual_partial_minor: costs.actual_minor,
            remaining_partial_minor: costs.remaining_minor,
            unresolved_actual_lines: costs.unresolved_actual_lines,
            unresolved_remaining_lines: costs.unresolved_remaining_lines,
            unknown_expense_collections: expenses.map(|e| e.unknown_task_collections),
        });
    }
    Ok(Curve { version: 1, samples, date_mode, native_parity_verified: false })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn model() -> ProjectModel {
        ProjectModel::from_metadata(&serde_json::json!({"schema_version":4,"tasks":[],"resources":[],"definition_storage":{}})).unwrap()
    }
    #[test]
    fn endpoints_extremes_and_single_evaluator_agree() {
        let model = model();
        let curve = sample(&model, i64::MIN, i64::MAX, 3, DateMode::Planned, || false).unwrap();
        assert_eq!(curve.samples.first().unwrap().focus_wallclock, i64::MIN);
        assert_eq!(curve.samples.last().unwrap().focus_wallclock, i64::MAX);
        for point in curve.samples {
            let frame = model.evaluate(point.focus_wallclock, DateMode::Planned);
            assert_eq!(point.planned_value_minor, frame.costs.planned_cumulative_minor);
            assert_eq!(point.unresolved_planned_lines, frame.costs.unresolved_cumulative_lines);
            assert_eq!(point.recorded_expenses_minor, frame.costs.recorded_actual_expenses.and_then(|e| e.recorded_at_focus_minor));
        }
    }
    #[test]
    fn curve_keeps_recorded_expenses_distinct_from_forecast_and_missing_actuals() {
        let data = serde_json::json!({"schema_version":4,"tasks":[{
            "guid":"task", "name":"Task", "planned_start_wallclock_scalar":0,
            "duration_storage_scalar":100,"status_storage_value":2,
            "date_storage_windows":[0,32535244799i64,100,32535244799i64],"assignments":[],
            "direct_costs":[{"guid":"budget","name":"Budget","amount_storage":120.,"type_storage_value":1}],
            "actual_expense_coverage":{"progress_collection_present":true,"report_collection_complete":true,
                "report_count":1,"item_count":1,"policy":"observed_type5_report_with_fixed_cost_items"},
            "actual_expense_reports":[{"report_guid":"report","date_storage_scalar":50,"label_storage":"Expense",
                "report_duration_storage_scalar":0,"type_storage_value":5,"items":[{
                    "item_guid":"expense","name":"Expense","amount_storage":30.,"type_storage_value":1,
                    "definition_reference_guid_storage":"00000000-0000-0000-0000-000000000000"}]}]
        }],"resources":[],"definition_storage":{}});
        let model = ProjectModel::from_metadata(&data).unwrap();
        let curve = sample(&model, 0, 100, 3, DateMode::Best, || false).unwrap();
        // Without a decoded calendar the planned interval stays unresolved, even
        // though the explicit dated expense ledger is independently usable.
        assert!(curve.samples.iter().all(|s| s.planned_value_minor == 0 && s.unresolved_planned_lines == 1));
        assert_eq!(curve.samples.iter().map(|s| s.recorded_expenses_minor).collect::<Vec<_>>(), vec![Some(0), Some(3000), Some(3000)]);
        assert!(curve.samples.iter().all(|s| s.direct_budget_minor == 12000 && s.resource_forecast_minor == 0));
        assert!(curve.samples.iter().all(|s| s.unresolved_actual_lines > 0 && s.actual_partial_minor == 0));
        assert!(!curve.native_parity_verified);
        let mut missing = data;
        missing["tasks"][0].as_object_mut().unwrap().remove("actual_expense_coverage");
        missing["tasks"][0].as_object_mut().unwrap().remove("actual_expense_reports");
        let model = ProjectModel::from_metadata(&missing).unwrap();
        let curve = sample(&model, 0, 100, 2, DateMode::Best, || false).unwrap();
        assert!(curve.samples.iter().all(|s| s.recorded_expenses_minor.is_none() && s.unknown_expense_collections == Some(1)));
        // Cancellation after work has begun must return no partial Curve.
        let mut calls = 0;
        assert!(
            sample(&model, 0, 100, 10, DateMode::Best, || {
                calls += 1;
                calls > 5
            })
            .is_err()
        );
    }
    #[test]
    fn refuses_bad_ranges_caps_and_cancellation() {
        let model = model();
        for (start, end, count) in [(0, 0, 2), (2, 1, 2), (0, 1, 1), (0, 1, 513)] {
            assert!(sample(&model, start, end, count, DateMode::Best, || false).is_err());
        }
        assert!(sample(&model, 0, 100, 2, DateMode::Best, || true).is_err());
    }
}
