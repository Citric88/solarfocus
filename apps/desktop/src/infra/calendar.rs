#![cfg(feature = "calendar")]
//! v1.3 Wave C — read-only macOS calendar awareness via EventKit.
//!
//! On macOS, EventKit gives us a unified view across iCloud, Google
//! (synced via the macOS Calendar app), Exchange, and Local accounts
//! through a single API. We never write events, never persist titles,
//! never transmit anything off-device.
//!
//! The algorithm half (free-block finder, "next event" lookup) is pure
//! and unit-testable without a live calendar; the EventKit half is
//! gated behind `#[cfg(target_os = "macos")]` so non-Mac builds at
//! least compile under `--features calendar`.

use chrono::{DateTime, Duration as CDur, Local, NaiveTime, TimeZone};

#[derive(Debug, Clone)]
pub struct CalendarEvent {
    pub title: String,
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
    pub source: String,
}

#[derive(Debug, Clone, Copy)]
pub struct FreeBlock {
    pub start: DateTime<Local>,
    pub end: DateTime<Local>,
}

impl FreeBlock {
    pub fn duration_minutes(&self) -> u32 {
        let delta = self.end - self.start;
        let mins = delta.num_minutes();
        if mins < 0 { 0 } else { mins as u32 }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CalendarError {
    #[error("calendar permission denied")]
    PermissionDenied,
    #[error("calendar unavailable: {0}")]
    Unavailable(String),
}

/// Pure interval algorithm — sort + merge overlapping events, then
/// scan gaps from `now` to `end_of_day`. Returns the earliest gap of
/// at least `min_minutes`. Used by both the live EventKit reader and
/// the test suite (where it's fed mock events).
pub fn find_next_free_block(
    events: &[CalendarEvent],
    now: DateTime<Local>,
    min_minutes: u32,
) -> Option<FreeBlock> {
    let end_of_day = end_of_today(now);
    if min_minutes == 0 || end_of_day <= now {
        return None;
    }

    // Filter events that intersect [now, end_of_day], then merge.
    let mut intervals: Vec<(DateTime<Local>, DateTime<Local>)> = events
        .iter()
        .filter(|e| e.end > now && e.start < end_of_day)
        .map(|e| (e.start.max(now), e.end.min(end_of_day)))
        .collect();
    intervals.sort_by_key(|(s, _)| *s);

    let mut merged: Vec<(DateTime<Local>, DateTime<Local>)> = Vec::with_capacity(intervals.len());
    for (s, e) in intervals {
        if let Some(last) = merged.last_mut() {
            if s <= last.1 {
                if e > last.1 {
                    last.1 = e;
                }
                continue;
            }
        }
        merged.push((s, e));
    }

    // Scan the gaps.
    let mut cursor = now;
    for (s, e) in &merged {
        if *s > cursor {
            let gap_end = *s;
            if (gap_end - cursor) >= CDur::minutes(min_minutes as i64) {
                return Some(FreeBlock { start: cursor, end: gap_end });
            }
        }
        if *e > cursor {
            cursor = *e;
        }
    }
    if (end_of_day - cursor) >= CDur::minutes(min_minutes as i64) {
        return Some(FreeBlock { start: cursor, end: end_of_day });
    }
    None
}

/// Earliest event whose end is in the future.
pub fn next_event<'a>(events: &'a [CalendarEvent], now: DateTime<Local>) -> Option<&'a CalendarEvent> {
    events
        .iter()
        .filter(|e| e.end > now)
        .min_by_key(|e| e.start)
}

fn end_of_today(now: DateTime<Local>) -> DateTime<Local> {
    let date = now.date_naive();
    let end = NaiveTime::from_hms_opt(23, 59, 59).unwrap();
    Local
        .from_local_datetime(&date.and_time(end))
        .single()
        .unwrap_or(now)
}

// --- macOS EventKit reader ---------------------------------------------
//
// v1.3.0 ships the pure algorithm + a manual "next deadline" text input
// in the UI. Live EventKit access (iCloud / Google / Exchange / Local
// via `objc2-event-kit`) lands in v1.3.1 once the binding's selector
// surface is fully exercised. The interface below is the contract the
// live reader will satisfy.
pub trait CalendarSource {
    fn events_today(&self) -> Result<Vec<CalendarEvent>, CalendarError>;
}

/// v1.3.0 fallback: a single user-typed deadline turned into a fake
/// 30-minute event so `find_next_free_block` and `next_event` work
/// the same way against it as they will against live EventKit data.
pub struct ManualDeadlineSource {
    pub label: String,
    pub when: DateTime<Local>,
}

impl CalendarSource for ManualDeadlineSource {
    fn events_today(&self) -> Result<Vec<CalendarEvent>, CalendarError> {
        Ok(vec![CalendarEvent {
            title: self.label.clone(),
            start: self.when,
            end: self.when + chrono::Duration::minutes(30),
            source: "Manual".to_string(),
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> DateTime<Local> {
        let today = Local::now().date_naive();
        Local
            .from_local_datetime(&today.and_hms_opt(h, m, 0).unwrap())
            .single()
            .unwrap()
    }

    fn ev(title: &str, sh: u32, sm: u32, eh: u32, em: u32) -> CalendarEvent {
        CalendarEvent {
            title: title.to_string(),
            start: at(sh, sm),
            end: at(eh, em),
            source: "Test".to_string(),
        }
    }

    #[test]
    fn free_block_finds_earliest_gap_ge_duration() {
        let events = vec![
            ev("Standup", 9, 0, 9, 30),
            ev("Lunch", 12, 0, 13, 0),
        ];
        let now = at(8, 30);
        let block = find_next_free_block(&events, now, 25).unwrap();
        // The 8:30 → 9:00 gap is 30 min ≥ 25 min, so that's the answer.
        assert_eq!(block.start, at(8, 30));
        assert_eq!(block.end, at(9, 0));
    }

    #[test]
    fn free_block_skips_too_short_gap() {
        let events = vec![
            ev("Meeting A", 9, 0, 9, 50),
            ev("Meeting B", 10, 0, 11, 0), // only 10-min gap before
        ];
        let now = at(8, 50);
        // 8:50 → 9:00 = 10 min, too short for 25.
        // 9:50 → 10:00 = 10 min, too short.
        // 11:00 → end of day = plenty.
        let block = find_next_free_block(&events, now, 25).unwrap();
        assert_eq!(block.start, at(11, 0));
    }

    #[test]
    fn overlapping_events_merge_correctly() {
        let events = vec![
            ev("A", 10, 0, 11, 30),
            ev("B", 11, 0, 12, 0), // overlaps A
            ev("C", 12, 30, 13, 0),
        ];
        let now = at(9, 0);
        // Free: 9:00 → 10:00 (60 min), then 12:00 → 12:30 (30 min).
        let block = find_next_free_block(&events, now, 50).unwrap();
        assert_eq!(block.start, at(9, 0));
        assert_eq!(block.end, at(10, 0));
    }

    #[test]
    fn next_event_returns_earliest_future() {
        let events = vec![
            ev("Past", 7, 0, 8, 0),
            ev("Future A", 14, 0, 15, 0),
            ev("Future B", 11, 0, 12, 0),
        ];
        let now = at(9, 0);
        let next = next_event(&events, now).unwrap();
        assert_eq!(next.title, "Future B");
    }

    #[test]
    fn no_events_returns_full_day_block() {
        let now = at(10, 0);
        let block = find_next_free_block(&[], now, 60).unwrap();
        assert_eq!(block.start, at(10, 0));
    }
}
