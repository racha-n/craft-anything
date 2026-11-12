//! Framed Task report groups. Only observed type5 Fixed expense items have actual-cost semantics.
//! Empty ordinary groups retain progress storage; nonempty unknown families fail explicitly.
use super::*;
#[derive(Serialize)]
pub(super) struct ExpenseItem {
    pub offset: usize,
    pub bytes: usize,
    pub item_guid: String,
    pub name: String,
    pub amount_storage: f64,
    pub type_storage_value: u32,
    pub definition_reference_guid_storage: String,
    pub raw_record_hex: String,
}
#[derive(Serialize)]
pub(super) struct ExpenseReport {
    pub offset: usize,
    pub bytes: usize,
    pub report_guid: String,
    pub date_storage_scalar: u64,
    pub label_storage: String,
    pub ordinal_storage: u32,
    pub report_duration_storage_scalar: u64,
    pub type_storage_value: u32,
    pub items: Vec<ExpenseItem>,
}
#[derive(Serialize)]
pub(super) struct ExpenseCoverage {
    pub progress_collection_present: bool,
    pub report_collection_complete: bool,
    pub report_count: usize,
    pub item_count: usize,
    pub policy: &'static str,
}
pub(super) struct Collection {
    pub records: Vec<ProgressRecord>,
    pub reports: Vec<ExpenseReport>,
    pub coverage: ExpenseCoverage,
    pub extra: usize,
    pub records_used: usize,
}
fn item(data: &[u8], at: usize) -> Result<(ExpenseItem, usize)> {
    marker(data, at, &[0x9c, 0x0e, 0xe3, 0x04])?;
    marker(data, checked_add(at, 4)?, &[0; 8])?;
    let item_guid = guid_at(data, checked_add(at, 12)?)?;
    if bytes(data, checked_add(at, 12)?, 16)?.iter().all(|b| *b == 0) {
        return Err(fail("invalid_identity", "Actual expense item GUID is nil"));
    }
    let (name, name_end) = string_at(data, checked_add(at, 28)?)?;
    let amount_storage = finite_at(data, name_end)?;
    if amount_storage < 0. {
        return Err(fail("unsupported_value", "Negative actual expense requires a separately admitted credit policy"));
    }
    let type_storage_value = u32_at(data, checked_add(name_end, 8)?)?;
    if type_storage_value != 1 {
        return Err(fail("unsupported_layout", "Actual expense cost kind is not observed Fixed type1"));
    }
    let reference_at = checked_add(name_end, 12)?;
    let definition_reference_guid_storage = guid_at(data, reference_at)?;
    if bytes(data, reference_at, 16)?.iter().any(|b| *b != 0) {
        return Err(fail("missing_dependency", "Actual expense has a nonnil definition reference; GlobalExpenseDefs resolution is not admitted"));
    }
    marker(data, checked_add(name_end, 28)?, &[0x78, 0xe2, 0x44, 0xa1, 0, 0, 0, 0])?;
    let end = checked_add(name_end, 36)?;
    Ok((
        ExpenseItem {
            offset: at,
            bytes: end - at,
            item_guid,
            name,
            amount_storage,
            type_storage_value,
            definition_reference_guid_storage,
            raw_record_hex: hex(bytes(data, at, end - at)?),
        },
        end,
    ))
}
pub(super) fn parse(data: &[u8], at: usize, budget: usize, callback: &mut dyn FnMut(f32) -> bool) -> Result<Collection> {
    check(callback, 0.5)?;
    let flag = *bytes(data, at, 1)?.first().ok_or_else(|| fail("truncated", "Missing progress/report collection flag"))?;
    let mut out = Collection {
        records: Vec::new(),
        reports: Vec::new(),
        coverage: ExpenseCoverage {
            progress_collection_present: flag == 1,
            report_collection_complete: true,
            report_count: 0,
            item_count: 0,
            policy: "observed_type5_report_with_fixed_cost_items",
        },
        extra: 0,
        records_used: 0,
    };
    if flag == 0 {
        return Ok(out);
    }
    if flag != 1 {
        return Err(fail("unsupported_layout", "Unknown progress/report collection flag"));
    }
    let open = checked_add(at, 1)?;
    marker(data, open, OPEN)?;
    let count = count_at(data, checked_add(open, 4)?)?;
    if count > budget {
        return Err(fail("limit", "Report/progress groups exceed shared relationship budget"));
    }
    out.records_used = count;
    let mut cursor = checked_add(open, 8)?;
    let mut identities = BTreeSet::new();
    for _ in 0..count {
        check(callback, 0.5)?;
        let start = cursor;
        marker(data, start, &[0x32, 0xe6, 0xb5, 0x72])?;
        let guid = guid_at(data, checked_add(start, 12)?)?;
        let date = u64_at(data, checked_add(start, 44)?)?;
        let duration = u64_at(data, checked_add(start, 52)?)?;
        let kind = u32_at(data, checked_add(start, 60)?)?;
        let (label, label_end) = string_at(data, checked_add(start, 64)?)?;
        marker(data, label_end, OPEN)?;
        let items = count_at(data, checked_add(label_end, 4)?)?;
        out.records_used = checked_add(out.records_used, items)?;
        if out.records_used > budget {
            return Err(fail("limit", "Actual expense items exceed shared relationship budget"));
        }
        if kind != 5 && items != 0 {
            return Err(fail("unsupported_layout", "Nested costs on a non-expense report have no admitted policy"));
        }
        if kind == 5 {
            marker(data, checked_add(start, 4)?, &[0; 8])?;
            marker(data, checked_add(start, 28)?, &[0; 16])?;
            if bytes(data, checked_add(start, 12)?, 16)?.iter().all(|b| *b == 0) || !identities.insert(guid.clone()) {
                return Err(fail("invalid_identity", "Actual expense report GUID is nil or duplicate"));
            }
        }
        cursor = checked_add(label_end, 8)?;
        let mut costs = Vec::new();
        for _ in 0..items {
            check(callback, 0.5)?;
            let (cost, end) = item(data, cursor)?;
            if !identities.insert(cost.item_guid.clone()) {
                return Err(fail("invalid_identity", "Duplicate actual expense identity"));
            }
            costs.push(cost);
            cursor = end;
        }
        marker(data, cursor, CLOSE)?;
        cursor = checked_add(cursor, 4)?;
        marker(data, cursor, &[0xce, 0xac, 0x72, 0xc9])?;
        cursor = checked_add(cursor, 4)?;
        let ordinal = u32_at(data, cursor)?;
        cursor = checked_add(cursor, 4)?;
        if kind == 5 {
            out.coverage.item_count = checked_add(out.coverage.item_count, costs.len())?;
            out.reports.push(ExpenseReport {
                offset: start,
                bytes: cursor - start,
                report_guid: guid,
                date_storage_scalar: date,
                label_storage: label,
                ordinal_storage: ordinal,
                report_duration_storage_scalar: duration,
                type_storage_value: kind,
                items: costs,
            });
        } else {
            out.records.push(ProgressRecord {
                offset: start,
                bytes: cursor - start,
                opaque_guid: guid,
                date_storage_scalar: date,
                duration_storage_scalar: duration,
                type_storage_value: kind,
                label_storage: label,
                ordinal_storage: ordinal,
            });
        }
    }
    marker(data, cursor, CLOSE)?;
    out.extra = checked_add(cursor, 4)?.checked_sub(open).ok_or_else(|| fail("limit", "Report length underflow"))?;
    out.coverage.report_count = out.reports.len();
    Ok(out)
}
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    fn text(out: &mut Vec<u8>, value: &str) {
        let units = value.encode_utf16().collect::<Vec<_>>();
        out.extend([255, 254, 255, units.len() as u8]);
        for unit in units {
            out.extend(unit.to_le_bytes());
        }
    }
    fn cost(amount: f64, reference: bool) -> Vec<u8> {
        let mut b = vec![0x9c, 0x0e, 0xe3, 0x04];
        b.extend([0; 8]);
        b.extend([7; 16]);
        text(&mut b, "Synthetic item");
        b.extend(amount.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend([u8::from(reference); 16]);
        b.extend([0x78, 0xe2, 0x44, 0xa1, 0, 0, 0, 0]);
        b
    }
    fn group(kind: u32, costs: &[Vec<u8>], label: &str) -> Vec<u8> {
        let mut b = vec![0x32, 0xe6, 0xb5, 0x72];
        b.extend([0; 8]);
        b.extend([3; 16]);
        b.extend([0; 16]);
        b.extend(1_794_182_400u64.to_le_bytes());
        b.extend(0u64.to_le_bytes());
        b.extend(kind.to_le_bytes());
        text(&mut b, label);
        b.extend(OPEN);
        b.extend((costs.len() as u32).to_le_bytes());
        for c in costs {
            b.extend(c);
        }
        b.extend(CLOSE);
        b.extend([0xce, 0xac, 0x72, 0xc9]);
        b.extend(9u32.to_le_bytes());
        b
    }
    fn collection(groups: &[Vec<u8>]) -> Vec<u8> {
        let mut b = vec![1];
        b.extend(OPEN);
        b.extend((groups.len() as u32).to_le_bytes());
        for g in groups {
            b.extend(g);
        }
        b.extend(CLOSE);
        b
    }
    #[test]
    fn expense_date_amount_unicode_and_progress_are_separate() {
        let raw = collection(&[group(2, &[], "ordinary"), group(5, &[cost(30., false)], "รายงาน")]);
        let parsed = parse(&raw, 0, 10, &mut |_| true).unwrap();
        assert_eq!(parsed.records.len(), 1);
        assert_eq!(parsed.records[0].type_storage_value, 2);
        let r = &parsed.reports[0];
        assert_eq!(r.label_storage, "รายงาน");
        assert_eq!(r.date_storage_scalar, 1_794_182_400);
        assert_eq!(r.items[0].amount_storage, 30.);
        assert_eq!(r.ordinal_storage, 9);
        assert_eq!(parsed.records_used, 3);
        assert_eq!(parsed.extra, raw.len() - 1);
        assert!(parsed.coverage.report_collection_complete);
    }
    #[test]
    fn explicit_absent_empty_and_empty_expense_report_are_distinct() {
        let absent = parse(&[0], 0, 10, &mut |_| true).unwrap();
        assert!(!absent.coverage.progress_collection_present);
        assert!(absent.coverage.report_collection_complete);
        let empty = collection(&[]);
        let parsed = parse(&empty, 0, 10, &mut |_| true).unwrap();
        assert!(parsed.coverage.progress_collection_present);
        assert_eq!(parsed.coverage.report_count, 0);
        let raw = collection(&[group(5, &[], "")]);
        let parsed = parse(&raw, 0, 10, &mut |_| true).unwrap();
        assert_eq!(parsed.coverage.report_count, 1);
        assert_eq!(parsed.coverage.item_count, 0);
    }
    #[test]
    fn truncated_unknown_refs_negative_nonfinite_unknown_kind_reject() {
        let raw = collection(&[group(5, &[cost(30., false)], "")]);
        for end in 0..raw.len() {
            assert!(parse(&raw[..end], 0, 10, &mut |_| true).is_err(), "cut{end}");
        }
        for amount in [-1., f64::NAN, f64::INFINITY] {
            let raw = collection(&[group(5, &[cost(amount, false)], "")]);
            assert!(parse(&raw, 0, 10, &mut |_| true).is_err());
        }
        let raw = collection(&[group(5, &[cost(30., true)], "")]);
        assert_eq!(parse(&raw, 0, 10, &mut |_| true).err().unwrap().code, "missing_dependency");
        let raw = collection(&[group(2, &[cost(30., false)], "")]);
        assert!(parse(&raw, 0, 10, &mut |_| true).is_err());
        let mut c = cost(30., false);
        let at = c.len() - 28;
        c[at..at + 4].copy_from_slice(&99u32.to_le_bytes());
        let raw = collection(&[group(5, &[c], "")]);
        assert!(parse(&raw, 0, 10, &mut |_| true).is_err());
    }
    #[test]
    fn shared_counts_duplicates_and_cancellation_are_bounded() {
        let raw = collection(&[group(5, &[cost(30., false)], "")]);
        assert!(parse(&raw, 0, 1, &mut |_| true).is_err());
        let raw = collection(&[group(5, &[cost(30., false), cost(31., false)], "")]);
        assert!(parse(&raw, 0, 10, &mut |_| true).is_err());
        let raw = collection(&[group(99, &[], "opaque")]);
        assert_eq!(parse(&raw, 0, 10, &mut |_| true).unwrap().records[0].type_storage_value, 99);
        assert_eq!(parse(&[0], 0, 10, &mut |_| false).err().unwrap().code, "cancelled");
        let raw = collection(&[group(5, &[cost(30., false)], "")]);
        let mut calls = 0;
        assert_eq!(
            parse(&raw, 0, 10, &mut |_| {
                calls += 1;
                calls < 3
            })
            .err()
            .unwrap()
            .code,
            "cancelled"
        );
    }
}
