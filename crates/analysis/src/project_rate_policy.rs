//! Bounded budget formulas for explicitly normalized quantities and one rate.
//!
//! Time-resource quantities are millionths of a nominal day, never elapsed days
//! or an average of calendar working segments. Callers own rate-window selection,
//! quantity normalization, evidence provenance and any aggregation. These helpers
//! do not derive actual expenditure, status, progress or cumulative time phases.

/// Scope to retain alongside results; this is not a general native-parity claim.
pub const POLICY_EVIDENCE: &str = "Declared monetary policy with explicit nominal-day normalization; corroborated by isolated eight-hour-day monetary controls and ten-hour-day quantity-display controls. This does not prove normalization of every source unit, rate crossing, actual expense or time phase.";

pub const MAX_AMOUNT_MINOR: i64 = 1_000_000_000_000;
pub const MAX_UNITS_MILLIONTHS: u64 = 1_000_000_000_000_000;
const MILLION: i128 = 1_000_000;
const HOUR_SECONDS: i128 = 3_600;

/// Evaluate one assignment's budget at one rate, in the supplied currency's cents.
///
/// Kinds 1/2/3 with unit 0 use nominal-day quantities: Fixed(1), Hourly Partial(2),
/// Daily(3), Hourly Complete(8). Material kind 4 with unit 0 or 7 admits Fixed(1)
/// and Per Unit(9), where quantities are millionths of the declared material unit.
/// Hourly policies require an explicit nominal day of 1..=86400 seconds. No
/// calendar inference or default eight-hour day is performed. Any supplied nominal
/// day is validated, including for formulas that do not need it.
///
/// Nonnegative rational amounts round once at the final cent, with exact halves
/// rounded upward. Hourly Complete first rounds TOTAL normalized hours upward to
/// a whole hour, then multiplies by the rate; it does not round each calendar day.
/// Fixed applies once, even for zero quantity; it is a budget, not actual spend.
pub fn budget(
    kind_storage: u8,
    unit_storage: Option<u8>,
    type_storage: u32,
    amount_minor: i64,
    normalized_units_millionths: u64,
    nominal_day_seconds: Option<u64>,
) -> Result<i64, String> {
    if !(0..=MAX_AMOUNT_MINOR).contains(&amount_minor) {
        return Err(format!("Rate amount must be between zero and {MAX_AMOUNT_MINOR} minor units"));
    }
    if normalized_units_millionths > MAX_UNITS_MILLIONTHS {
        return Err(format!("Normalized quantity exceeds {MAX_UNITS_MILLIONTHS} millionths; split or correct the quantity"));
    }
    if nominal_day_seconds.is_some_and(|seconds| !(1..=86_400).contains(&seconds)) {
        return Err("Explicit nominal day must be between 1 and 86400 seconds".into());
    }
    let time_family = matches!(kind_storage, 1..=3);
    let supported = if time_family {
        unit_storage == Some(0) && matches!(type_storage, 1 | 2 | 3 | 8)
    } else if kind_storage == 4 {
        matches!(unit_storage, Some(0 | 7)) && matches!(type_storage, 1 | 9)
    } else {
        false
    };
    if !supported {
        return Err(format!(
            "Unsupported budget basis: resource kind {kind_storage}, unit {unit_storage:?}, rate type {type_storage}; supply a verified or separately declared normalization policy"
        ));
    }
    let amount = i128::from(amount_minor);
    let quantity = i128::from(normalized_units_millionths);
    let (numerator, denominator) = match type_storage {
        1 => (amount, 1),
        3 | 9 => (multiply(amount, quantity)?, MILLION),
        2 | 8 => {
            let seconds = nominal_day_seconds
                .ok_or_else(|| "Hourly rates require explicit nominal_day_seconds; do not infer it from averaged working segments".to_string())?;
            let normalized_seconds = multiply(quantity, i128::from(seconds))?;
            let hour_denominator = multiply(MILLION, HOUR_SECONDS)?;
            if type_storage == 8 {
                let hours = ceil_ratio(normalized_seconds, hour_denominator)?;
                (multiply(amount, hours)?, 1)
            } else {
                (multiply(amount, normalized_seconds)?, hour_denominator)
            }
        }
        // Exhaustive for the supported table above, without assuming input validity.
        _ => return Err("Rate type has no declared budget formula".into()),
    };
    let rounded = numerator.checked_add(denominator / 2).and_then(|value| value.checked_div(denominator)).ok_or_else(arithmetic_error)?;
    i64::try_from(rounded).map_err(|_| "Budget exceeds signed 64-bit minor units; split or correct the rate and quantity".into())
}

fn multiply(left: i128, right: i128) -> Result<i128, String> {
    left.checked_mul(right).ok_or_else(arithmetic_error)
}

fn ceil_ratio(numerator: i128, denominator: i128) -> Result<i128, String> {
    numerator.checked_add(denominator - 1).and_then(|value| value.checked_div(denominator)).ok_or_else(arithmetic_error)
}

fn arithmetic_error() -> String {
    "Budget arithmetic exceeded its checked numeric range; reduce the rate or quantity".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_day_control_predictions_keep_fixed_hourly_and_daily_distinct() {
        assert_eq!(budget(2, Some(0), 1, 1_200, 3_000_000, None), Ok(1_200));
        assert_eq!(budget(2, Some(0), 2, 1_200, 3_000_000, Some(28_800)), Ok(28_800));
        for kind in [1, 2, 3] {
            assert_eq!(budget(kind, Some(0), 3, 1_200, 3_000_000, None), Ok(3_600));
        }
    }

    #[test]
    fn fractional_control_predictions_round_total_hours_but_not_days() {
        let quantity = 187_500; // 1.5 hours under an explicitly declared eight-hour day.
        assert_eq!(budget(2, Some(0), 2, 1_200, quantity, Some(28_800)), Ok(1_800));
        assert_eq!(budget(2, Some(0), 8, 1_200, quantity, Some(28_800)), Ok(2_400));
        assert_eq!(budget(2, Some(0), 3, 1_200, quantity, None), Ok(225));
    }

    #[test]
    fn hourly_complete_has_exact_zero_integer_and_first_fraction_boundaries() {
        assert_eq!(budget(2, Some(0), 8, 1_200, 0, Some(28_800)), Ok(0));
        assert_eq!(budget(2, Some(0), 8, 1_200, 125_000, Some(28_800)), Ok(1_200));
        assert_eq!(budget(2, Some(0), 8, 1_200, 125_001, Some(28_800)), Ok(2_400));
        assert_eq!(budget(2, Some(0), 8, 1_200, 1, Some(1)), Ok(1_200));
    }

    #[test]
    fn nearest_cent_is_applied_once_to_the_final_nonnegative_rational() {
        assert_eq!(budget(1, Some(0), 3, 1, 499_999, None), Ok(0));
        assert_eq!(budget(1, Some(0), 3, 1, 500_000, None), Ok(1));
        assert_eq!(budget(1, Some(0), 3, 1, 500_001, None), Ok(1));
        assert_eq!(budget(2, Some(0), 2, 3, 500_000, Some(1_800)), Ok(1));
    }

    #[test]
    fn fixed_budget_is_once_per_assignment_and_not_quantity_scaled() {
        for kind in [1, 2, 3, 4] {
            assert_eq!(budget(kind, Some(0), 1, 99, 0, None), Ok(99));
            assert_eq!(budget(kind, Some(0), 1, 99, MAX_UNITS_MILLIONTHS, None), Ok(99));
        }
    }

    #[test]
    fn material_per_unit_uses_only_an_explicitly_admitted_unit() {
        for unit in [0, 7] {
            assert_eq!(budget(4, Some(unit), 9, 125, 2_500_000, None), Ok(313));
        }
        assert!(budget(4, Some(1), 9, 125, 2_500_000, None).is_err());
        assert!(budget(4, None, 9, 125, 2_500_000, None).is_err());
    }

    #[test]
    fn nominal_day_is_explicit_and_not_an_implicit_eight_hour_default() {
        for rate_type in [2, 8] {
            assert!(budget(2, Some(0), rate_type, 0, 0, None).is_err());
        }
        assert_eq!(budget(1, Some(0), 2, 100, 1_000_000, Some(36_000)), Ok(1_000));
        assert_eq!(budget(1, Some(0), 2, 100, 1_000_000, Some(28_800)), Ok(800));
        assert_eq!(budget(1, Some(0), 2, 100, 1_000_000, Some(86_400)), Ok(2_400));
    }

    #[test]
    fn bad_nominal_days_fail_even_when_formula_does_not_need_them() {
        for seconds in [0, 86_401, u64::MAX] {
            assert!(budget(2, Some(0), 1, 0, 0, Some(seconds)).is_err());
        }
    }

    #[test]
    fn unknown_basis_never_disappears_behind_zero_amount_or_quantity() {
        for (kind, unit, rate) in [(0, Some(0), 1), (5, Some(0), 1), (1, None, 1), (1, Some(7), 1), (1, Some(0), 9), (4, Some(0), 2), (1, Some(0), u32::MAX)] {
            assert!(budget(kind, unit, rate, 0, 0, Some(28_800)).is_err());
        }
    }

    #[test]
    fn negative_rates_and_input_caps_are_rejected_before_arithmetic() {
        for amount in [-1, i64::MIN, MAX_AMOUNT_MINOR + 1, i64::MAX] {
            assert!(budget(2, Some(0), 1, amount, 0, None).is_err());
        }
        assert!(budget(2, Some(0), 1, 0, MAX_UNITS_MILLIONTHS + 1, None).is_err());
        assert!(budget(2, Some(0), 1, 0, u64::MAX, None).is_err());
        assert_eq!(budget(2, Some(0), 1, MAX_AMOUNT_MINOR, MAX_UNITS_MILLIONTHS, None), Ok(MAX_AMOUNT_MINOR));
    }

    #[test]
    fn valid_inputs_can_exceed_output_capacity_without_panicking() {
        for rate_type in [2, 3, 8] {
            let error = budget(2, Some(0), rate_type, MAX_AMOUNT_MINOR, MAX_UNITS_MILLIONTHS, Some(86_400)).unwrap_err();
            assert!(error.contains("64-bit"));
        }
        assert_eq!(multiply(i128::MAX, 2), Err(arithmetic_error()));
        assert_eq!(ceil_ratio(i128::MAX, 2), Err(arithmetic_error()));
    }
}
