//! Recorded Fixed expense reports, independent of budgets and earned-value forecasts.
//! Point-date accumulation is a declared policy over naive source wallclock storage.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

const MAX_RECORDS: usize = 10_000;
const MAX_TEXT: usize = 4 * 1024 * 1024;
const MAX_LABEL: usize = 4096;
const UNSET_DATE: u64 = 32_535_244_799;
const POLICY: &str = "observed_type5_report_with_fixed_cost_items";
const NIL: &str = "00000000-0000-0000-0000-000000000000";
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpenseLine {
    pub task_guid: String,
    pub report_guid: String,
    pub item_guid: String,
    pub name: String,
    pub date_wallclock: Option<i64>,
    pub report_duration_storage_scalar: u64,
    pub amount_storage: f64,
    pub amount_minor: Option<i64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpenseSummary {
    pub recorded_total_minor: Option<i64>,
    pub recorded_at_focus_minor: Option<i64>,
    pub partial_recorded_total_minor: i64,
    pub partial_recorded_at_focus_minor: i64,
    pub report_count: usize,
    pub item_count: usize,
    pub unknown_task_collections: usize,
    pub unsupported_time_reports: usize,
    pub items: Vec<ExpenseLine>,
    pub limitations: Vec<String>,
}
#[derive(Debug)]
pub struct Ledger {
    items: Vec<ExpenseLine>,
    report_count: usize,
    unknown_task_collections: usize,
    unsupported_time_reports: usize,
}
fn check(cancelled: &mut impl FnMut() -> bool) -> Result<(), String> {
    if cancelled() { Err("Recorded expense work cancelled".into()) } else { Ok(()) }
}
fn count(value: &Value, name: &str) -> Result<usize, String> {
    value
        .get(name)
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .filter(|n| *n <= MAX_RECORDS)
        .ok_or_else(|| format!("Invalid or oversized expense {name}"))
}
fn integer(value: &Value, name: &str) -> Result<u64, String> {
    value.get(name).and_then(Value::as_u64).ok_or_else(|| format!("Invalid expense {name}"))
}
fn flag(value: &Value, name: &str) -> Result<bool, String> {
    value.get(name).and_then(Value::as_bool).ok_or_else(|| format!("Invalid expense {name}"))
}
fn charge(text: &mut usize, amount: usize) -> Result<(), String> {
    *text = text.checked_add(amount).filter(|n| *n <= MAX_TEXT).ok_or("Expense derived text exceeds four MiB")?;
    Ok(())
}
fn label<'a>(value: &'a Value, name: &str, text: &mut usize) -> Result<&'a str, String> {
    let result = value.get(name).and_then(Value::as_str).filter(|s| s.len() <= MAX_LABEL).ok_or_else(|| format!("Invalid or oversized expense {name}"))?;
    charge(text, result.len())?;
    Ok(result)
}
fn identity<'a>(value: &'a Value, name: &str, text: &mut usize, ids: &mut BTreeSet<&'a str>) -> Result<&'a str, String> {
    let result = label(value, name, text)?;
    if result.is_empty() || !ids.insert(result) {
        return Err("Expense source identities must be nonempty and unique".into());
    }
    Ok(result)
}
fn add_records(records: &mut usize, added: usize) -> Result<(), String> {
    *records = records.checked_add(added).filter(|n| *n <= MAX_RECORDS).ok_or("Expense aggregate record/item count exceeds10000")?;
    Ok(())
}
fn date(report: &Value) -> Result<Option<i64>, String> {
    let Some(value) = report.get("date_storage_scalar").filter(|v| !v.is_null()) else { return Ok(None) };
    let scalar = value.as_u64().ok_or("Expense report date is not unsigned wallclock storage")?;
    Ok((scalar != UNSET_DATE).then(|| i64::try_from(scalar).ok()).flatten())
}
fn minor(amount: f64) -> Option<i64> {
    (amount.is_finite() && (0.0..=10_000_000_000.0).contains(&amount)).then(|| (amount * 100.).round() as i64)
}
impl Ledger {
    pub fn from_metadata(metadata: &Value, mut cancelled: impl FnMut() -> bool) -> Result<Self, String> {
        check(&mut cancelled)?;
        if metadata.get("schema_version").and_then(Value::as_u64) != Some(4) {
            return Err("Expense ledger requires project metadata schema4".into());
        }
        let tasks = metadata.get("tasks").and_then(Value::as_array).ok_or("Expense ledger tasks are missing")?;
        let mut records = 0;
        add_records(&mut records, tasks.len())?;
        let mut text = 0;
        let mut ids = BTreeSet::new();
        let mut ledger = Self { items: Vec::new(), report_count: 0, unknown_task_collections: 0, unsupported_time_reports: 0 };
        for task in tasks {
            check(&mut cancelled)?;
            let task_guid = identity(task, "guid", &mut text, &mut ids)?;
            let coverage = task.get("actual_expense_coverage");
            let reports = task.get("actual_expense_reports");
            let mut expected = None;
            if let Some(coverage) = coverage {
                if !coverage.is_object() || coverage.get("policy").and_then(Value::as_str) != Some(POLICY) {
                    return Err("Expense coverage policy is absent or unsupported".into());
                }
                let present = flag(coverage, "progress_collection_present")?;
                let complete = flag(coverage, "report_collection_complete")?;
                let report_count = count(coverage, "report_count")?;
                let item_count = count(coverage, "item_count")?;
                if !present && (report_count != 0 || item_count != 0) {
                    return Err("Absent expense collection claims nonempty contents".into());
                }
                if !complete {
                    ledger.unknown_task_collections += 1;
                }
                expected = Some((report_count, item_count));
            } else {
                ledger.unknown_task_collections += 1;
            }
            let reports = match reports {
                Some(value) => value.as_array().ok_or("Expense reports must be an array")?,
                None if coverage.is_none() => continue,
                None => return Err("Expense coverage is present but its report array is missing".into()),
            };
            if expected.is_some_and(|counts| counts.0 != reports.len()) {
                return Err("Expense report coverage count disagrees with the framed collection".into());
            }
            add_records(&mut records, reports.len())?;
            ledger.report_count = ledger.report_count.checked_add(reports.len()).ok_or("Expense report count overflows")?;
            let mut task_items = 0usize;
            for report in reports {
                check(&mut cancelled)?;
                let report_guid = identity(report, "report_guid", &mut text, &mut ids)?;
                label(report, "label_storage", &mut text)?;
                if integer(report, "type_storage_value")? != 5 {
                    return Err("Expense report kind is unsupported; only type5 is admitted".into());
                }
                let duration = integer(report, "report_duration_storage_scalar")?;
                let date = date(report)?;
                if duration != 0 || date.is_none() {
                    ledger.unsupported_time_reports += 1;
                }
                let items = report.get("items").and_then(Value::as_array).ok_or("Expense report item array is missing")?;
                add_records(&mut records, items.len())?;
                task_items = task_items.checked_add(items.len()).ok_or("Expense item count overflows")?;
                ledger.items.try_reserve(items.len()).map_err(|_| "Cannot reserve bounded expense storage")?;
                for item in items {
                    check(&mut cancelled)?;
                    let item_guid = identity(item, "item_guid", &mut text, &mut ids)?;
                    let name = label(item, "name", &mut text)?;
                    let reference = label(item, "definition_reference_guid_storage", &mut text)?;
                    if reference != NIL {
                        return Err("Expense definition reference is not admitted nil storage".into());
                    }
                    if integer(item, "type_storage_value")? != 1 {
                        return Err("Expense item kind is unsupported; only Fixed type1 is admitted".into());
                    }
                    let amount = item
                        .get("amount_storage")
                        .and_then(Value::as_f64)
                        .filter(|n| n.is_finite() && *n >= 0.)
                        .ok_or("Expense amount must be finite and nonnegative")?;
                    charge(&mut text, task_guid.len())?;
                    charge(&mut text, report_guid.len())?;
                    ledger.items.push(ExpenseLine {
                        task_guid: task_guid.into(),
                        report_guid: report_guid.into(),
                        item_guid: item_guid.into(),
                        name: name.into(),
                        date_wallclock: date,
                        report_duration_storage_scalar: duration,
                        amount_storage: amount,
                        amount_minor: minor(amount),
                    });
                }
            }
            if expected.is_some_and(|counts| counts.1 != task_items) {
                return Err("Expense item coverage count disagrees with the framed collection".into());
            }
        }
        check(&mut cancelled)?;
        Ok(ledger)
    }
    pub fn evaluate(&self, focus: i64, mut cancelled: impl FnMut() -> bool) -> Result<ExpenseSummary, String> {
        check(&mut cancelled)?;
        let mut total = 0i64;
        let mut at_focus = 0i64;
        let mut total_complete = self.unknown_task_collections == 0;
        let mut focus_complete = total_complete;
        let mut items = Vec::new();
        items.try_reserve(self.items.len()).map_err(|_| "Cannot reserve bounded expense result")?;
        for item in &self.items {
            check(&mut cancelled)?;
            if let Some(amount) = item.amount_minor {
                total = total.checked_add(amount).ok_or("Recorded expense total overflows")?;
            } else {
                total_complete = false;
            }
            if item.amount_storage != 0. {
                match (item.report_duration_storage_scalar, item.date_wallclock) {
                    (0, Some(date)) if focus < date => {}
                    (0, Some(_)) => {
                        if let Some(amount) = item.amount_minor {
                            at_focus = at_focus.checked_add(amount).ok_or("Recorded expense focus total overflows")?;
                        } else {
                            focus_complete = false;
                        }
                    }
                    _ => focus_complete = false,
                }
            }
            items.push(item.clone());
        }
        let mut limitations=vec!["Recorded Fixed report expenses are separate from Budgeted Direct Costs; no budget attribution, forecast or Actual=planned-value assumption".into(),
            "Naive source wallclock point-date accumulation and per-item nearest-cent rounding; currency conversion and native project-wide parity are not established".into()];
        if self.unknown_task_collections > 0 {
            limitations.push("Missing or incomplete source expense coverage remains unknown; known subtotals exclude unseen records".into());
        }
        if !total_complete {
            limitations.push("Recorded total is unresolved; partial_recorded_total_minor contains only representable known amounts".into());
        }
        if self.unsupported_time_reports > 0 {
            limitations
                .push("Missing/unset report dates and nonzero durations have no admitted time allocation; zero amounts retain their additive identity".into());
        }
        if !focus_complete {
            limitations.push("Recorded amount at focus is unresolved; its partial subtotal excludes unsupported allocations".into());
        }
        check(&mut cancelled)?;
        Ok(ExpenseSummary {
            recorded_total_minor: total_complete.then_some(total),
            recorded_at_focus_minor: focus_complete.then_some(at_focus),
            partial_recorded_total_minor: total,
            partial_recorded_at_focus_minor: at_focus,
            report_count: self.report_count,
            item_count: self.items.len(),
            unknown_task_collections: self.unknown_task_collections,
            unsupported_time_reports: self.unsupported_time_reports,
            items,
            limitations,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;
    fn item(id: &str, amount: f64) -> Value {
        json!({"item_guid":id,"name":"Synthetic Fixed expense","amount_storage":amount,"type_storage_value":1,"definition_reference_guid_storage":NIL})
    }
    fn report(items: Vec<Value>, duration: u64) -> Value {
        json!({"report_guid":"report-a","date_storage_scalar":100,"label_storage":"Original synthetic report","report_duration_storage_scalar":duration,"type_storage_value":5,"items":items})
    }
    fn metadata(reports: Vec<Value>) -> Value {
        let count = reports.iter().filter_map(|r| r["items"].as_array()).map(Vec::len).sum::<usize>();
        json!({"schema_version":4,"tasks":[{"guid":"task-a","actual_expense_reports":reports,
            "actual_expense_coverage":{"progress_collection_present":true,"report_collection_complete":true,"report_count":reports.len(),"item_count":count,"policy":POLICY}}]})
    }
    fn evaluate(data: &Value, focus: i64) -> ExpenseSummary {
        Ledger::from_metadata(data, || false).unwrap().evaluate(focus, || false).unwrap()
    }
    #[test]
    fn explicit_empty_and_missing_collections_are_not_conflated() {
        let mut explicit = metadata(vec![]);
        explicit["tasks"][0]["actual_expense_coverage"]["progress_collection_present"] = json!(false);
        let empty = evaluate(&explicit, 100);
        assert_eq!(empty.recorded_total_minor, Some(0));
        assert_eq!(empty.recorded_at_focus_minor, Some(0));
        let absent = json!({"schema_version":4,"tasks":[{"guid":"task-a"}]});
        let unknown = evaluate(&absent, 100);
        assert_eq!(unknown.recorded_total_minor, None);
        assert_eq!(unknown.recorded_at_focus_minor, None);
        assert_eq!(unknown.unknown_task_collections, 1);
        assert_eq!(unknown.partial_recorded_total_minor, 0);
        let mut incomplete = metadata(vec![report(vec![item("item-a", 30.)], 0)]);
        incomplete["tasks"][0]["actual_expense_coverage"]["report_collection_complete"] = json!(false);
        let result = evaluate(&incomplete, 100);
        assert_eq!(result.recorded_total_minor, None);
        assert_eq!(result.partial_recorded_total_minor, 3000);
    }
    #[test]
    fn thirty_is_recorded_once_at_its_point_date() {
        let data = metadata(vec![report(vec![item("item-a", 30.)], 0)]);
        for (focus, expected) in [(99, 0), (100, 3000), (101, 3000)] {
            let result = evaluate(&data, focus);
            assert_eq!(result.recorded_total_minor, Some(3000));
            assert_eq!(result.recorded_at_focus_minor, Some(expected));
            assert_eq!((result.report_count, result.item_count, result.unknown_task_collections), (1, 1, 0));
            assert_eq!(result.items[0].amount_storage, 30.);
            assert_eq!(result.items[0].date_wallclock, Some(100));
            let restored: ExpenseSummary = serde_json::from_value(serde_json::to_value(&result).unwrap()).unwrap();
            assert_eq!(restored.recorded_total_minor, Some(3000));
        }
    }
    #[test]
    fn unsupported_time_keeps_overall_actual_but_never_invents_allocation() {
        let data = metadata(vec![report(vec![item("item-a", 30.)], 3600)]);
        let result = evaluate(&data, 200);
        assert_eq!(result.recorded_total_minor, Some(3000));
        assert_eq!(result.recorded_at_focus_minor, None);
        assert_eq!(result.unsupported_time_reports, 1);
        let mut missing = metadata(vec![report(vec![item("item-a", 30.)], 0)]);
        missing["tasks"][0]["actual_expense_reports"][0].as_object_mut().unwrap().remove("date_storage_scalar");
        assert_eq!(evaluate(&missing, 200).recorded_at_focus_minor, None);
        missing["tasks"][0]["actual_expense_reports"][0]["date_storage_scalar"] = json!(UNSET_DATE);
        assert_eq!(evaluate(&missing, 200).recorded_at_focus_minor, None);
        missing["tasks"][0]["actual_expense_reports"][0]["date_storage_scalar"] = json!(u64::MAX);
        assert_eq!(evaluate(&missing, 200).recorded_at_focus_minor, None);
        let mut zero = metadata(vec![report(vec![item("item-a", 0.)], 3600)]);
        zero["tasks"][0]["actual_expense_reports"][0]["date_storage_scalar"] = Value::Null;
        assert_eq!(evaluate(&zero, 200).recorded_at_focus_minor, Some(0));
    }
    #[test]
    fn unknown_collections_and_unrepresentable_amounts_preserve_partial_totals() {
        let mut data = metadata(vec![report(vec![item("item-a", 30.)], 0)]);
        data["tasks"].as_array_mut().unwrap().push(json!({"guid":"task-unknown"}));
        let result = evaluate(&data, 100);
        assert_eq!(result.recorded_total_minor, None);
        assert_eq!(result.recorded_at_focus_minor, None);
        assert_eq!(result.partial_recorded_total_minor, 3000);
        assert_eq!(result.partial_recorded_at_focus_minor, 3000);
        let data = metadata(vec![report(vec![item("item-a", 30.), item("item-huge", 1e308)], 0)]);
        let result = evaluate(&data, 100);
        assert_eq!(result.recorded_total_minor, None);
        assert_eq!(result.partial_recorded_total_minor, 3000);
        assert_eq!(result.recorded_at_focus_minor, None);
        assert_eq!(result.items[1].amount_storage, 1e308);
        assert!(result.items[1].amount_minor.is_none());
        assert_eq!(evaluate(&data, 99).recorded_at_focus_minor, Some(0));
    }
    #[test]
    fn amounts_are_rounded_per_item_within_a_declared_bound() {
        let result = evaluate(&metadata(vec![report(vec![item("item-a", 0.005), item("item-b", 0.005)], 0)]), 100);
        assert_eq!(result.recorded_total_minor, Some(2));
        assert_eq!(result.recorded_at_focus_minor, Some(2));
        assert_eq!(minor(10_000_000_000.), Some(1_000_000_000_000));
        assert_eq!(minor(10_000_000_001.), None);
    }
    #[test]
    fn malformed_counts_identities_references_and_values_fail() {
        let base = metadata(vec![report(vec![item("item-a", 30.)], 0)]);
        for path in ["report_count", "item_count"] {
            let mut data = base.clone();
            data["tasks"][0]["actual_expense_coverage"][path] = json!(2);
            assert!(Ledger::from_metadata(&data, || false).is_err());
        }
        let mut data = base.clone();
        data["tasks"][0]["actual_expense_coverage"]["progress_collection_present"] = json!(false);
        assert!(Ledger::from_metadata(&data, || false).is_err());
        let mut data = base.clone();
        data["tasks"][0]["actual_expense_reports"][0]["items"][0]["item_guid"] = json!("report-a");
        assert!(Ledger::from_metadata(&data, || false).is_err());
        for (field, value) in [
            ("amount_storage", json!(-1.)),
            ("amount_storage", Value::Null),
            ("amount_storage", json!("NaN")),
            ("type_storage_value", json!(2)),
            ("definition_reference_guid_storage", json!("unresolved-reference")),
            ("name", json!("x".repeat(MAX_LABEL + 1))),
        ] {
            let mut data = base.clone();
            data["tasks"][0]["actual_expense_reports"][0]["items"][0][field] = value;
            assert!(Ledger::from_metadata(&data, || false).is_err(), "{field}");
        }
        let mut data = base.clone();
        data["tasks"][0]["actual_expense_reports"][0]["type_storage_value"] = json!(99);
        assert!(Ledger::from_metadata(&data, || false).is_err());
        let mut data = base.clone();
        data["tasks"][0]["actual_expense_reports"][0]["date_storage_scalar"] = json!("tomorrow");
        assert!(Ledger::from_metadata(&data, || false).is_err());
        let mut data = base.clone();
        data["tasks"][0].as_object_mut().unwrap().remove("actual_expense_reports");
        assert!(Ledger::from_metadata(&data, || false).is_err());
        let mut data = base;
        let duplicate = data["tasks"][0].clone();
        data["tasks"].as_array_mut().unwrap().push(duplicate);
        assert!(Ledger::from_metadata(&data, || false).is_err());
    }
    #[test]
    fn aggregate_count_text_and_cancellation_budgets_are_enforced() {
        let too_many = json!({"schema_version":4,"tasks":vec![json!({"guid":"t"});MAX_RECORDS+1]});
        assert!(Ledger::from_metadata(&too_many, || false).is_err());
        let items = (0..1200)
            .map(|n| {
                let mut item = item(&format!("i{n}"), 1.);
                item["name"] = json!("x".repeat(MAX_LABEL));
                item
            })
            .collect();
        assert!(Ledger::from_metadata(&metadata(vec![report(items, 0)]), || false).is_err());
        let too_many_items = metadata(vec![report(vec![item("same", 1.); MAX_RECORDS], 0)]);
        assert!(Ledger::from_metadata(&too_many_items, || false).is_err());
        let data = metadata(vec![report(vec![item("item-a", 30.), item("item-b", 10.)], 0)]);
        assert!(Ledger::from_metadata(&data, || true).is_err());
        let mut calls = 0;
        assert!(
            Ledger::from_metadata(&data, || {
                calls += 1;
                calls > 3
            })
            .is_err()
        );
        let ledger = Ledger::from_metadata(&data, || false).unwrap();
        assert!(ledger.evaluate(100, || true).is_err());
        let mut calls = 0;
        assert!(
            ledger
                .evaluate(100, || {
                    calls += 1;
                    calls > 2
                })
                .is_err()
        );
    }
}
