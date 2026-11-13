//! Bounded backward allocation in the same naive working-time coordinate as forward allocation.
use super::*;

impl Calendars {
    /// Subtract positive work from a finish boundary. Non-working gaps before the
    /// boundary are skipped, so Wednesday 09:00 and Tuesday 17:00 can coincide.
    /// Zero work preserves the supplied instant, as in forward allocation.
    pub fn working_start_before_with_budget(
        &self,
        calendar_guid: Option<&str>,
        finish_wallclock: i64,
        duration_seconds: u64,
        budget: WorkingBudget,
        mut cancelled: impl FnMut() -> bool,
    ) -> std::result::Result<i64, CalendarError> {
        if cancelled() {
            return Err(CalendarError::Cancelled);
        }
        if duration_seconds > MAX_WORK_SECONDS || budget.max_scanned_days > MAX_SCAN_DAYS || budget.max_segments > MAX_SEGMENTS {
            return Err(CalendarError::Limit { budget: "working-time request/budget".into() });
        }
        if finish_wallclock == UNSPECIFIED_WALLCLOCK {
            return Err(CalendarError::Unsupported { reason: "unspecified wall-clock scalar".into() });
        }
        let index = self.index().map_err(|e| CalendarError::InvalidData { reason: e.to_string() })?;
        let id = calendar_guid.or(self.default_calendar_guid.as_deref()).ok_or(CalendarError::MissingCalendar { guid: None })?;
        let calendar = index.calendars.get(id).copied().ok_or_else(|| CalendarError::MissingCalendar { guid: Some(id.into()) })?;
        if duration_seconds == 0 {
            return Ok(finish_wallclock);
        }
        let mut day = finish_wallclock.div_euclid(DAY);
        let mut remaining = duration_seconds;
        let mut segments = 0usize;
        for _ in 0..budget.max_scanned_days {
            if cancelled() {
                return Err(CalendarError::Cancelled);
            }
            let midnight = day.checked_mul(DAY).ok_or(CalendarError::DateOverflow)?;
            let (_, kind) = index.day_type(calendar, midnight, &mut cancelled)?;
            if kind.kind_storage == 1 {
                if kind.intervals.is_empty() {
                    return Err(CalendarError::Unsupported { reason: "working day without intervals".into() });
                }
                for interval in kind.intervals.iter().rev() {
                    if cancelled() {
                        return Err(CalendarError::Cancelled);
                    }
                    let a = midnight.checked_add(i64::from(interval.start_second)).ok_or(CalendarError::DateOverflow)?;
                    let b = midnight.checked_add(i64::from(interval.end_second)).ok_or(CalendarError::DateOverflow)?.min(finish_wallclock);
                    if a >= b {
                        continue;
                    }
                    if segments >= budget.max_segments {
                        return Err(CalendarError::Limit { budget: "working segments".into() });
                    }
                    segments += 1;
                    let available = u64::try_from(b.checked_sub(a).ok_or(CalendarError::DateOverflow)?).map_err(|_| CalendarError::DateOverflow)?;
                    if remaining <= available {
                        return b.checked_sub(i64::try_from(remaining).map_err(|_| CalendarError::DateOverflow)?).ok_or(CalendarError::DateOverflow);
                    }
                    remaining -= available;
                }
            } else if kind.kind_storage != 2 {
                return Err(CalendarError::Unsupported { reason: "unobserved working-day type".into() });
            }
            day = day.checked_sub(1).ok_or(CalendarError::DateOverflow)?;
        }
        Err(CalendarError::Limit { budget: "scanned calendar days".into() })
    }
}
