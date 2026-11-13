use super::*;
use project_calendar::{Calendar, Conventions, DateException, DayType, WorkingInterval};

const CAL: &str = "11111111-1111-1111-1111-111111111111";
const WORK: &str = "22222222-2222-2222-2222-222222222222";
const OFF: &str = "33333333-3333-3333-3333-333333333333";
const ANCHOR: i64 = 1794128400; // Sunday 2026-11-08 09:00, naive clock.

fn calendars() -> Calendars {
    Calendars {
        schema_version: 1,
        source_bytes: 0,
        raw_header_hex: String::new(),
        root_calendar_guids: vec![CAL.into()],
        default_calendar_guid: Some(CAL.into()),
        conventions: Conventions::default(),
        records: vec![Calendar {
            offset: 0,
            guid: CAL.into(),
            name: "Synthetic calendar".into(),
            parent_guid: None,
            weekday_day_type_guids: std::array::from_fn(|n| Some(if n == 0 || n == 6 { OFF } else { WORK }.into())),
            exceptions: vec![],
            ordinal_storage: 0,
            day_types: vec![
                DayType {
                    offset: 0,
                    guid: WORK.into(),
                    name: "Work".into(),
                    color_storage: [0.; 3],
                    kind_storage: 1,
                    intervals: vec![WorkingInterval { raw_start_wallclock: 118800, raw_end_wallclock: 147600, start_second: 32400, end_second: 61200 }],
                    ordinal_storage: 0,
                },
                DayType { offset: 0, guid: OFF.into(), name: "Off".into(), color_storage: [0.; 3], kind_storage: 2, intervals: vec![], ordinal_storage: 1 },
            ],
        }],
    }
}
fn task(id: &str, predecessors: &[(&str, i64)]) -> Task {
    Task {
        guid: id.into(),
        name: id.into(),
        parent_guid: None,
        calendar_guid_storage: CAL.into(),
        planned_start_wallclock_scalar: 1794214800,
        duration_storage_scalar: 28800,
        status_storage_value: 1,
        links: predecessors
            .iter()
            .map(|(id, lag)| Link {
                predecessor_guid: (*id).into(),
                type_storage_window: 1,
                lag_storage_hex: lag.to_le_bytes().iter().map(|b| format!("{b:02x}")).collect(),
            })
            .collect(),
    }
}
#[test]
fn fs_preview_uses_working_boundaries_and_positive_lag() {
    for (lag, start, finish) in [(0, 1794301200, 1794330000), (57600, 1794474000, 1794502800)] {
        // Reverse input order also verifies that output identity/order is independent of traversal order.
        let tasks = [task("b", &[("a", lag)]), task("a", &[])];
        let p = calculate(&calendars(), &tasks, ANCHOR, true, &mut || false).unwrap();
        assert_eq!(p.tasks[0].guid, "b");
        assert_eq!(p.tasks[0].proposed_start_wallclock, start);
        assert_eq!(p.tasks[0].proposed_finish_wallclock, finish);
        assert_eq!(p.tasks[1].proposed_start_wallclock, 1794214800);
        assert!(!p.native_parity_verified);
        assert!(!p.critical_path_calculated);
    }
}
#[test]
fn fs_preview_takes_latest_predecessor_and_calendar_exception() {
    let mut a = task("a", &[]);
    a.duration_storage_scalar = 57600;
    let p = calculate(&calendars(), &[task("b", &[("a", 0), ("c", 0)]), a, task("c", &[])], ANCHOR, true, &mut || false).unwrap();
    assert_eq!(p.tasks[0].proposed_start_wallclock, 1794387600); // Wednesday.
    let mut c = calendars();
    c.records[0].exceptions.push(DateException { offset: 0, date_wallclock: 1794268800, day_type_guid: OFF.into(), ordinal_storage: 0 });
    let p = calculate(&c, &[task("a", &[]), task("b", &[("a", 0)])], ANCHOR, true, &mut || false).unwrap();
    assert_eq!(p.tasks[1].proposed_start_wallclock, 1794387600);
}
#[test]
fn fs_preview_rejects_unsupported_inputs_without_partial_results() {
    let c = calendars();
    assert!(calculate(&c, &[task("a", &[])], ANCHOR, false, &mut || false).is_err());
    assert!(calculate(&c, &[task("a", &[])], ANCHOR + 1, true, &mut || false).is_err());
    for tasks in [
        vec![task("a", &[("missing", 0)])],
        vec![task("a", &[("a", 0)])],
        vec![task("a", &[("b", 0)]), task("b", &[("a", 0)])],
        vec![task("a", &[]), task("A", &[])],
        vec![task("a", &[]), task("b", &[("a", -28800)])],
    ] {
        assert!(calculate(&c, &tasks, ANCHOR, true, &mut || false).is_err());
    }
    let mut b = task("b", &[("a", 0)]);
    b.links[0].type_storage_window = 3;
    assert!(calculate(&c, &[task("a", &[]), b], ANCHOR, true, &mut || false).is_err());
    for invalid in [0, 1, 1_000_001] {
        let mut a = task("a", &[]);
        a.duration_storage_scalar = invalid;
        assert!(calculate(&c, &[a], ANCHOR, true, &mut || false).is_err());
    }
    let mut a = task("a", &[]);
    a.parent_guid = Some("parent".into());
    assert!(calculate(&c, &[a], ANCHOR, true, &mut || false).is_err());
    let mut a = task("a", &[]);
    a.status_storage_value = 2;
    assert!(calculate(&c, &[a], ANCHOR, true, &mut || false).is_err());
    assert!(calculate(&c, &[task("a", &[])], ANCHOR, true, &mut || true).is_err());
}
#[test]
fn fs_preview_checks_metadata_schema_counts_and_calendar_presence() {
    assert!(from_metadata(&serde_json::json!({}), ANCHOR, true, || false).is_err());
    assert!(from_metadata(&serde_json::json!({"schema_version":4,"tasks":[]}), ANCHOR, true, || false).is_err());
    let values = vec![serde_json::json!({}); 513];
    assert!(from_metadata(&serde_json::json!({"schema_version":4,"tasks":values}), ANCHOR, true, || false).unwrap_err().contains("512"));
}

#[test]
fn fs_preview_uses_upstream_calendar_for_lag_and_task_calendar_for_duration() {
    let mut c = calendars();
    let other = "44444444-4444-4444-4444-444444444444";
    let work = "55555555-5555-5555-5555-555555555555";
    c.root_calendar_guids.push(other.into());
    c.records.push(Calendar {
        offset: 0,
        guid: other.into(),
        name: "Continuous".into(),
        parent_guid: None,
        weekday_day_type_guids: std::array::from_fn(|_| Some(work.into())),
        exceptions: vec![],
        ordinal_storage: 1,
        day_types: vec![DayType {
            offset: 0,
            guid: work.into(),
            name: "Continuous work".into(),
            color_storage: [0.; 3],
            kind_storage: 1,
            intervals: vec![WorkingInterval { raw_start_wallclock: 86400, raw_end_wallclock: 172800, start_second: 0, end_second: 86400 }],
            ordinal_storage: 0,
        }],
    });
    let mut b = task("b", &[("a", 28800)]);
    b.calendar_guid_storage = other.into();
    let p = calculate(&c, &[task("a", &[]), b], 1794560400, true, &mut || false).unwrap(); // Friday morning.
    assert_eq!(p.tasks[1].effective_calendar_guid, other);
    assert_eq!(p.tasks[1].proposed_start_wallclock, 1794848400); // Monday 17:00 after upstream lag.
    assert_eq!(p.tasks[1].proposed_finish_wallclock, 1794877200); // Tuesday 01:00 in continuous calendar.
}

#[test]
fn fs_preview_enforces_shared_working_time_budget() {
    let mut ticks = TICK_LIMIT;
    let error = allocation(&calendars(), Some(CAL), ANCHOR, 60, &mut ticks, &mut || false).unwrap_err();
    assert!(error.contains("budget"));
}

#[test]
fn expanded_relationships_match_unequal_duration_control_dates() {
    for (kind, lag, start, finish) in [
        (3, 0, 1794214800, 1794243600),      // SS: Monday.
        (4, 0, 1794301200, 1794330000),      // FF: Tuesday.
        (1, -28800, 1794301200, 1794330000), // FS minus one workday.
        (1, 0, 1794387600, 1794416400),      // FS: Wednesday.
        (1, 28800, 1794474000, 1794502800),  // FS plus one: Thursday.
        (3, -28800, 1794214800, 1794243600), // SS lead is clipped by anchor.
        (3, 28800, 1794301200, 1794330000),
        (4, -28800, 1794214800, 1794243600),
        (4, 28800, 1794387600, 1794416400),
        (2, -28800, 1794214800, 1794243600),
        (2, 0, 1794214800, 1794243600),
        (2, 28800, 1794214800, 1794243600),
        (2, 57600, 1794301200, 1794330000),
    ] {
        let mut a = task("a", &[]);
        a.duration_storage_scalar = 57600;
        let mut b = task("b", &[("a", lag)]);
        b.links[0].type_storage_window = kind;
        let p = calculate_model(&calendars(), &[b, a], ANCHOR, true, ScheduleModel::FourRelationshipsSignedLag, &mut || false).unwrap();
        assert_eq!(p.version, 2);
        assert_eq!((p.tasks[0].proposed_start_wallclock, p.tasks[0].proposed_finish_wallclock), (start, finish));
        assert!(!p.native_parity_verified);
        assert!(!p.critical_path_calculated);
    }
}

#[test]
fn active_sf_uses_working_finish_boundary_across_overnight_gap() {
    let mut root = task("root", &[]);
    root.duration_storage_scalar = 57600;
    let middle = task("middle", &[("root", 0)]);
    let mut last = task("last", &[("middle", 0)]);
    last.links[0].type_storage_window = 2;
    let p = calculate_model(&calendars(), &[last, middle, root], ANCHOR, true, ScheduleModel::FourRelationshipsSignedLag, &mut || false).unwrap();
    assert_eq!(p.tasks[1].proposed_start_wallclock, 1794387600); // Wednesday 09:00.
    assert_eq!(p.tasks[0].proposed_start_wallclock, 1794301200); // Tuesday 09:00.
    assert_eq!(p.tasks[0].proposed_finish_wallclock, 1794330000); // Tuesday 17:00.
}

#[test]
fn expanded_model_retains_budgets_cancellation_and_unknown_type_refusal() {
    let mut b = task("b", &[("a", i64::MIN)]);
    assert!(calculate_model(&calendars(), &[task("a", &[]), b], ANCHOR, true, ScheduleModel::FourRelationshipsSignedLag, &mut || false).is_err());
    b = task("b", &[("a", 0)]);
    b.links[0].type_storage_window = 5;
    assert!(calculate_model(&calendars(), &[task("a", &[]), b], ANCHOR, true, ScheduleModel::FourRelationshipsSignedLag, &mut || false).is_err());
    assert!(reverse_allocation(&calendars(), Some(CAL), ANCHOR, 60, &mut 0, &mut || true).is_err());
    let mut ticks = TICK_LIMIT;
    assert!(reverse_allocation(&calendars(), Some(CAL), ANCHOR, 60, &mut ticks, &mut || false).unwrap_err().contains("budget"));
}
