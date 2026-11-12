//! Versioned CSV presentation choices. Raw snapshots remain independent of selected CSVs.
use craft_anything_evidence::ToolError;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Table {
    Tasks,
    Resources,
    Costs,
    ActualExpenses,
    Phases,
}
impl Table {
    pub fn file(self) -> &'static str {
        match self {
            Self::Tasks => "tasks.csv",
            Self::Resources => "resources.csv",
            Self::Costs => "costs.csv",
            Self::ActualExpenses => "actual_expenses.csv",
            Self::Phases => "phases.csv",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub version: u32,
    pub name: String,
    pub tables: Vec<Table>,
    pub readable_dates: bool,
}
impl Template {
    pub fn validate(&self) -> Result<(), ToolError> {
        if self.version != 1 || self.name.trim().is_empty() || self.name.len() > 120 || self.name.chars().any(char::is_control) {
            return Err(ToolError::invalid("Report templates require version 1 and a visible name of at most 120 UTF-8 bytes."));
        }
        if self.tables.is_empty() || self.tables.len() > 5 || self.tables.iter().collect::<BTreeSet<_>>().len() != self.tables.len() {
            return Err(ToolError::invalid("Choose one to five distinct CSV tables in the report template."));
        }
        Ok(())
    }
}
pub fn presets() -> Vec<Template> {
    use Table::*;
    [
        ("All tables with dates", vec![Tasks, Resources, Costs, ActualExpenses, Phases]),
        ("Schedule review", vec![Tasks, Phases]),
        ("Cost review", vec![Resources, Costs, ActualExpenses]),
    ]
    .into_iter()
    .map(|(name, tables)| Template { version: 1, name: name.into(), tables, readable_dates: true })
    .collect()
}
pub fn resolve(template: Option<Template>, path: Option<&str>) -> Result<Option<Template>, ToolError> {
    let template = match (template, path) {
        (Some(_), Some(_)) => return Err(ToolError::invalid("Pass either template or template_path, not both.")),
        (Some(t), None) => Some(t),
        (None, Some(path)) => {
            let path = std::path::Path::new(path);
            if !path.is_absolute() {
                return Err(ToolError::invalid("Choose an absolute template JSON path."));
            }
            let bytes = craft_anything_platform::fs::read_file_capped(path, 16 * 1024)?;
            Some(serde_json::from_slice(&bytes).map_err(|e| ToolError::invalid(format!("Invalid report template: {e}")))?)
        }
        (None, None) => None,
    };
    if let Some(t) = &template {
        t.validate()?;
    }
    Ok(template)
}
fn year_start(year: i64) -> i64 {
    let y = year - 1;
    365 * y + y / 4 - y / 100 + y / 400 - 719_162
}
/// A naive displayed-clock scalar, with no UTC or timezone designation.
pub fn wallclock(value: i64) -> Option<String> {
    let days = value.div_euclid(86_400);
    if !(year_start(1600)..year_start(10_000)).contains(&days) {
        return None;
    }
    let (mut low, mut high) = (1600, 10_000);
    while high - low > 1 {
        let middle = (low + high) / 2;
        if year_start(middle) <= days {
            low = middle;
        } else {
            high = middle;
        }
    }
    let leap = low % 4 == 0 && (low % 100 != 0 || low % 400 == 0);
    let mut day = days - year_start(low);
    let mut month = 1;
    for count in [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31] {
        if day < count {
            break;
        }
        day -= count;
        month += 1;
    }
    let second = value.rem_euclid(86_400);
    Some(format!("{low:04}-{month:02}-{:02} {:02}:{:02}:{:02}", day + 1, second / 3600, second % 3600 / 60, second % 60))
}
pub fn display(value: Option<i64>) -> Value {
    value.map(|v| json!(wallclock(v).unwrap_or_else(|| "Unresolved".into()))).unwrap_or(Value::Null)
}
pub fn add_dates(table: Table, rows: &mut [Value]) {
    for row in rows {
        match table {
            Table::Tasks => {
                for field in ["planned_start", "planned_finish", "best_start", "best_finish"] {
                    let value = display(row.get(field).and_then(Value::as_i64));
                    row[format!("{field}_display")] = value;
                }
            }
            Table::ActualExpenses => row["date_wallclock_display"] = display(row.get("date_wallclock").and_then(Value::as_i64)),
            Table::Resources => {
                let labels = row
                    .get("rates")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .map(|rate| display(rate.get("effective_wallclock").and_then(Value::as_i64)))
                    .collect::<Vec<_>>();
                row["rate_effective_dates_display"] = json!(labels);
            }
            Table::Costs | Table::Phases => {}
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dates_handle_epoch_negative_leap_centuries_and_extremes() {
        assert_eq!(wallclock(0).as_deref(), Some("1970-01-01 00:00:00"));
        assert_eq!(wallclock(-1).as_deref(), Some("1969-12-31 23:59:59"));
        assert_eq!(wallclock(951_827_696).as_deref(), Some("2000-02-29 12:34:56"));
        assert_eq!(wallclock(year_start(1900) * 86400 + 59 * 86400).as_deref(), Some("1900-03-01 00:00:00"));
        assert_eq!(wallclock(year_start(1600) * 86400).as_deref(), Some("1600-01-01 00:00:00"));
        assert_eq!(wallclock(year_start(10_000) * 86400 - 1).as_deref(), Some("9999-12-31 23:59:59"));
        assert!(wallclock(i64::MIN).is_none());
        assert!(wallclock(i64::MAX).is_none());
    }
    #[test]
    fn bad_templates_are_rejected_and_dates_keep_raw_values() {
        let mut t = presets().remove(0);
        t.tables.push(Table::Tasks);
        assert!(t.validate().is_err());
        t.tables.pop();
        t.version = 2;
        assert!(t.validate().is_err());
        let mut rows = vec![json!({"planned_start":-1,"planned_finish":null,"best_start":i64::MAX,"best_finish":0})];
        add_dates(Table::Tasks, &mut rows);
        assert_eq!(rows[0]["planned_start"], -1);
        assert_eq!(rows[0]["planned_finish_display"], Value::Null);
        assert_eq!(rows[0]["best_start_display"], "Unresolved");
    }
}
