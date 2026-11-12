//! Explicit empty-expense handling is additive accounting, not a paid resource formula.
//! An absent rate list is unknown. At least one parsed rate group is required, and every
//! group's successfully parsed expense array must be empty. Date/unit normalization then
//! cannot change the sum of expenses. Native fidelity remains separately unverified.
use super::ResourceCostView;

pub(super) const EMPTY_EXPENSE_POLICY: &str = "Declared additive-cost policy: all parsed expense lists are explicitly empty, so budget/actual/remaining/cumulative costs are zero; native parity is not established by this policy";

pub(super) fn explicitly_empty(resource: &ResourceCostView) -> bool {
    !resource.rates.is_empty() && resource.rates.iter().all(|rate| rate.items.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_model::{AppearancePolicy, DateMode, ProjectModel};
    use serde_json::{Value, json};

    fn metadata(kind: u8) -> Value {
        json!({"schema_version":4,"tasks":[{"guid":"task","name":"Work","planned_start_wallclock_scalar":100,"duration_storage_scalar":10,"status_storage_value":2,"date_storage_windows":[100,200,200,32535244799i64],"assignments":[{"resource_guid":"resource","profile_guid":"profile","planned_units":2.0}]}],
            "resources":[{"guid":"resource","name":"Assigned resource","kind_storage_byte":kind,"unit_storage_byte":0,"cost_storage_groups":[{"raw_header_hex":"00000000000000000000000000000000000000000000000000000000000000000000000000000000","items":[]}]}],
            "definition_storage":{"profile":{"section":"Use Profiles","name":"Install"}}})
    }
    #[test]
    fn explicit_empty_expenses_are_zero_without_guessing_resource_units_or_calendar() {
        for kind in [1, 2, 3, 4] {
            let model = ProjectModel::from_metadata(&metadata(kind)).unwrap();
            assert!(explicitly_empty(&model.resources[0]));
            let frame = model.evaluate_with_policy(150, DateMode::Best, AppearancePolicy::Strict, || false).unwrap();
            let line = &frame.costs.lines[0];
            assert_eq!((line.budget_minor, line.actual_minor, line.remaining_minor, line.planned_cumulative_minor), (Some(0), Some(0), Some(0), Some(0)));
            assert_eq!((line.actual_units, line.remaining_units), (None, None));
            assert!(line.unresolved_reasons.iter().any(|reason| reason == EMPTY_EXPENSE_POLICY));
            assert!(!frame.native_parity_verified);
        }
    }
    #[test]
    fn absent_groups_and_missing_items_do_not_become_zero() {
        let mut absent = metadata(4);
        absent["resources"][0]["cost_storage_groups"] = json!([]);
        let model = ProjectModel::from_metadata(&absent).unwrap();
        assert!(!explicitly_empty(&model.resources[0]));
        assert_eq!(model.evaluate(150, DateMode::Best).costs.lines[0].budget_minor, None);
        absent["resources"][0]["cost_storage_groups"] = json!([{"raw_header_hex":"00"}]);
        assert!(ProjectModel::from_metadata(&absent).is_err());
    }
    #[test]
    fn mixed_empty_and_nonempty_windows_remain_unresolved() {
        let mut data = metadata(4);
        data["resources"][0]["cost_storage_groups"].as_array_mut().unwrap().push(json!({"raw_header_hex":"00000000000000000000000000000000000000000000000000000000000100000000000000000000","items":[{"description":"Rate","amount_storage":3.25,"type_storage":9}]}));
        let model = ProjectModel::from_metadata(&data).unwrap();
        assert!(!explicitly_empty(&model.resources[0]));
        assert_eq!(model.evaluate(150, DateMode::Best).costs.lines[0].budget_minor, None);
    }
    #[test]
    fn invalid_planned_units_remain_rejected_even_when_expenses_are_empty() {
        let mut data = metadata(4);
        data["tasks"][0]["assignments"][0]["planned_units"] = json!(-2.0);
        let model = ProjectModel::from_metadata(&data).unwrap();
        let frame = model.evaluate(150, DateMode::Best);
        assert_eq!(frame.costs.lines[0].budget_minor, None);
        assert!(frame.costs.lines[0].unresolved_reasons.iter().any(|reason| reason.contains("valid units")));
    }
    #[test]
    fn invalid_actual_and_remaining_units_are_not_erased_by_the_zero_basis() {
        let mut data = metadata(4);
        data["tasks"][0]["assignments"][0]["actual_units_storage"] = json!(-1.0);
        data["tasks"][0]["assignments"][0]["remaining_units_storage"] = json!(2_000_000_000.0);
        let model = ProjectModel::from_metadata(&data).unwrap();
        let frame = model.evaluate(150, DateMode::Best);
        assert_eq!(frame.costs.lines[0].budget_minor, Some(0));
        assert_eq!(frame.costs.lines[0].actual_minor, None);
        assert_eq!(frame.costs.lines[0].remaining_minor, None);
        assert!(frame.costs.lines[0].unresolved_reasons.iter().any(|reason| reason.contains("invalid")));
    }
}
