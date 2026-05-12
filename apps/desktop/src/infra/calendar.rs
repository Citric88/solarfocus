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

// --- Calendar source contract ------------------------------------------

pub trait CalendarSource {
    fn events_today(&self) -> Result<Vec<CalendarEvent>, CalendarError>;
}

/// Manual fallback used when EventKit access is denied or unavailable.
/// A single user-typed deadline turned into a fake 30-minute event so
/// `find_next_free_block` and `next_event` work uniformly against it.
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

// --- v2.1 — ICS file reader (cross-platform live calendar) -------------
//
// Reads a local `.ics` file the user pointed at via Setup → General.
// Works on macOS, Windows, and Linux without any native API binding —
// every major calendar provider exports to ICS:
//   - Outlook: File → Save Calendar → .ics
//   - Google Calendar: Settings → Export → .zip with one .ics per
//     calendar, or any single calendar's "Public URL" → .ics
//   - Apple Calendar: File → Export → .ics
//   - iCloud / Exchange / Office 365: subscribe URL → publishes .ics
//
// Privacy contract: file lives on disk where the user put it. We never
// download from the publish URL automatically; the user is responsible
// for keeping it fresh (drop-in or auto-sync via the user's tool of
// choice).
pub struct IcsFileSource {
    pub path: std::path::PathBuf,
}

impl IcsFileSource {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

impl CalendarSource for IcsFileSource {
    fn events_today(&self) -> Result<Vec<CalendarEvent>, CalendarError> {
        let body = std::fs::read_to_string(&self.path).map_err(|e| {
            CalendarError::Unavailable(format!("Cannot read .ics: {e}"))
        })?;
        let now = Local::now();
        let day_start = Local
            .from_local_datetime(&now.date_naive().and_hms_opt(0, 0, 0).unwrap())
            .single()
            .unwrap_or(now);
        let day_end = end_of_today(now);
        Ok(parse_ics_events(&body, day_start, day_end))
    }
}

/// Pure parser for ICS bodies. Only extracts what we need:
/// SUMMARY, DTSTART, DTEND. Skips everything else (RRULE, ATTENDEE,
/// VALARM, etc.) — SolarFocus only cares about "what's blocking the
/// next 24h". Returns events whose [start, end] intersects
/// [filter_start, filter_end].
///
/// Format support: VEVENT blocks with VALUE=DATE-TIME starts/ends in
/// either UTC (`Z` suffix) or floating-local. Recurring events (RRULE)
/// are NOT expanded — only the master DTSTART is reported. This is
/// honest minimum-viable; richer support is a follow-up.
pub fn parse_ics_events(
    body: &str,
    filter_start: DateTime<Local>,
    filter_end: DateTime<Local>,
) -> Vec<CalendarEvent> {
    let mut out = Vec::new();
    let mut in_event = false;
    let mut summary: Option<String> = None;
    let mut dtstart: Option<DateTime<Local>> = None;
    let mut dtend: Option<DateTime<Local>> = None;

    // ICS uses CRLF line continuations: a line starting with space or
    // tab continues the previous line. Unfold first.
    let unfolded = unfold_ics(body);

    for line in unfolded.lines() {
        let line = line.trim_end();
        if line == "BEGIN:VEVENT" {
            in_event = true;
            summary = None;
            dtstart = None;
            dtend = None;
            continue;
        }
        if line == "END:VEVENT" {
            if let (Some(s), Some(e)) = (dtstart, dtend) {
                if e > filter_start && s < filter_end {
                    out.push(CalendarEvent {
                        title: summary.clone().unwrap_or_else(|| "(sin título)".to_string()),
                        start: s,
                        end: e,
                        source: "ICS".to_string(),
                    });
                }
            } else if let Some(s) = dtstart {
                // No DTEND → assume 30-minute event.
                let e = s + chrono::Duration::minutes(30);
                if e > filter_start && s < filter_end {
                    out.push(CalendarEvent {
                        title: summary.clone().unwrap_or_else(|| "(sin título)".to_string()),
                        start: s,
                        end: e,
                        source: "ICS".to_string(),
                    });
                }
            }
            in_event = false;
            continue;
        }
        if !in_event {
            continue;
        }
        // Property line: "NAME[;PARAM=VALUE...]:VALUE"
        let (name_part, value) = match line.split_once(':') {
            Some(t) => t,
            None => continue,
        };
        // Strip params from name.
        let name = name_part.split(';').next().unwrap_or(name_part);
        match name {
            "SUMMARY" => {
                summary = Some(unescape_ics(value));
            }
            "DTSTART" => {
                dtstart = parse_ics_datetime(value);
            }
            "DTEND" => {
                dtend = parse_ics_datetime(value);
            }
            _ => {}
        }
    }
    out.sort_by_key(|e| e.start);
    out
}

fn unfold_ics(body: &str) -> String {
    // Per RFC 5545: lines starting with " " or "\t" continue the prev.
    let mut out = String::with_capacity(body.len());
    for raw in body.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.starts_with(' ') || line.starts_with('\t') {
            out.push_str(&line[1..]);
        } else {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(line);
        }
    }
    out
}

fn unescape_ics(s: &str) -> String {
    // RFC 5545 §3.3.11: \\, \;, \,, \N, \n
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') | Some('N') => out.push('\n'),
                Some(';') => out.push(';'),
                Some(',') => out.push(','),
                Some('\\') => out.push('\\'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn parse_ics_datetime(value: &str) -> Option<DateTime<Local>> {
    // Two formats we accept:
    //   YYYYMMDDTHHMMSSZ   → UTC
    //   YYYYMMDDTHHMMSS    → floating local
    //   YYYYMMDD           → date-only, treat as local midnight
    let v = value.trim();
    if v.len() == 16 && v.ends_with('Z') {
        // 20260508T143000Z
        let utc = chrono::NaiveDateTime::parse_from_str(&v[..15], "%Y%m%dT%H%M%S").ok()?;
        Some(chrono::Utc.from_utc_datetime(&utc).with_timezone(&Local))
    } else if v.len() == 15 {
        // 20260508T143000
        let naive = chrono::NaiveDateTime::parse_from_str(v, "%Y%m%dT%H%M%S").ok()?;
        Local.from_local_datetime(&naive).single()
    } else if v.len() == 8 {
        // 20260508 — date only
        let date = chrono::NaiveDate::parse_from_str(v, "%Y%m%d").ok()?;
        let naive = date.and_hms_opt(0, 0, 0)?;
        Local.from_local_datetime(&naive).single()
    } else {
        None
    }
}

// --- macOS EventKit live reader (v1.3.1) -------------------------------

#[cfg(target_os = "macos")]
pub mod ek {
    use super::{CalendarError, CalendarEvent, CalendarSource};
    use chrono::{DateTime, Duration as CDur, Local, NaiveTime, TimeZone};
    use std::sync::{Arc, Mutex};

    /// Seconds between Unix epoch (1970-01-01) and macOS reference
    /// date (2001-01-01) — used for NSDate ↔ chrono conversion.
    const UNIX_TO_REF: f64 = 978_307_200.0;

    /// Live read-only EventKit reader. Wraps an `EKEventStore` and
    /// pulls today's events across **all** synced calendars (iCloud,
    /// Google synced via Calendar.app, Exchange, Local) in one query.
    pub struct CalendarReader {
        store: objc2::rc::Retained<objc2_event_kit::EKEventStore>,
        access_granted: Arc<Mutex<Option<bool>>>,
    }

    impl CalendarReader {
        pub fn new() -> Self {
            // SAFETY: EKEventStore::new is a standard alloc + init.
            let store = unsafe { objc2_event_kit::EKEventStore::new() };
            Self {
                store,
                access_granted: Arc::new(Mutex::new(None)),
            }
        }

        /// Request full read access. Blocks the calling thread until
        /// the user responds via the macOS permission prompt — caller
        /// must NOT run this on the UI thread.
        pub fn request_access(&self) -> Result<bool, CalendarError> {
            use objc2::runtime::Bool;
            let granted = self.access_granted.clone();
            let barrier = Arc::new(std::sync::Barrier::new(2));
            let barrier_cb = barrier.clone();

            let block = block2::RcBlock::new(
                move |ok: Bool, _err: *mut objc2_foundation::NSError| {
                    *granted.lock().unwrap() = Some(ok.as_bool());
                    barrier_cb.wait();
                },
            );
            // SAFETY: requestFullAccessToEventsWithCompletion: schedules
            // the completion on a background queue. We park the calling
            // thread on a Barrier until it fires. The block stays alive
            // until the barrier is released.
            unsafe {
                let raw = &*block as *const block2::Block<dyn Fn(Bool, *mut objc2_foundation::NSError)>
                    as *mut _;
                self.store.requestFullAccessToEventsWithCompletion(raw);
            }
            barrier.wait();
            Ok(self.access_granted.lock().unwrap().unwrap_or(false))
        }
    }

    impl CalendarSource for CalendarReader {
        fn events_today(&self) -> Result<Vec<CalendarEvent>, CalendarError> {
            // [today 00:00:00 local, tomorrow 00:00:00 local)
            let today = Local::now().date_naive();
            let start_local = Local
                .from_local_datetime(&today.and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap()))
                .single()
                .ok_or_else(|| {
                    CalendarError::Unavailable("could not compute today start".into())
                })?;
            let end_local = start_local + CDur::days(1);

            let start_ns = to_ns_date(start_local);
            let end_ns = to_ns_date(end_local);

            // SAFETY: predicateForEvents:endDate:calendars: returns an
            // autoreleased NSPredicate. eventsMatchingPredicate: returns
            // an NSArray<EKEvent>.
            let events_array = unsafe {
                let predicate = self
                    .store
                    .predicateForEventsWithStartDate_endDate_calendars(
                        &start_ns, &end_ns, None,
                    );
                self.store.eventsMatchingPredicate(&predicate)
            };

            let mut out = Vec::with_capacity(events_array.len());
            for ev in &events_array {
                // SAFETY: title/startDate/endDate return non-null
                // Retained<> on returned events. calendar() may be None.
                unsafe {
                    let raw_title = ev.title().to_string();
                    let title = if raw_title.trim().is_empty() {
                        "(sin título)".to_string()
                    } else {
                        raw_title
                    };
                    let start = from_ns_date(&ev.startDate());
                    let end = from_ns_date(&ev.endDate());
                    let source = ev
                        .calendar()
                        .map(|c| c.title().to_string())
                        .unwrap_or_else(|| "Local".into());
                    out.push(CalendarEvent { title, start, end, source });
                }
            }
            Ok(out)
        }
    }

    fn to_ns_date(dt: DateTime<Local>) -> objc2::rc::Retained<objc2_foundation::NSDate> {
        let unix_secs = dt.timestamp() as f64
            + dt.timestamp_subsec_nanos() as f64 / 1e9;
        let interval = unix_secs - UNIX_TO_REF;
        unsafe {
            objc2_foundation::NSDate::dateWithTimeIntervalSinceReferenceDate(interval)
        }
    }

    fn from_ns_date(d: &objc2_foundation::NSDate) -> DateTime<Local> {
        let interval = unsafe { d.timeIntervalSinceReferenceDate() };
        let unix_secs = interval + UNIX_TO_REF;
        let secs = unix_secs.trunc() as i64;
        let nanos = ((unix_secs.fract()) * 1e9).abs() as u32;
        Local.timestamp_opt(secs, nanos).single().unwrap_or_else(Local::now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Timelike;
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

    // --- v2.1 ICS parser tests ---

    fn at_local(h: u32, m: u32) -> DateTime<Local> {
        let now = Local::now().date_naive();
        Local
            .from_local_datetime(&now.and_hms_opt(h, m, 0).unwrap())
            .single()
            .unwrap()
    }

    #[test]
    fn ics_parses_single_event_local() {
        let now = Local::now();
        let today = now.format("%Y%m%d").to_string();
        let body = format!(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nBEGIN:VEVENT\r\n\
             SUMMARY:Standup\r\nDTSTART:{today}T140000\r\n\
             DTEND:{today}T143000\r\nEND:VEVENT\r\nEND:VCALENDAR\r\n"
        );
        let day_start = at_local(0, 0);
        let day_end = at_local(23, 59);
        let events = parse_ics_events(&body, day_start, day_end);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Standup");
        assert_eq!(events[0].start.hour(), 14);
        assert_eq!(events[0].source, "ICS");
    }

    #[test]
    fn ics_unescapes_summary() {
        let now = Local::now();
        let today = now.format("%Y%m%d").to_string();
        let body = format!(
            "BEGIN:VEVENT\r\nSUMMARY:Hello\\, World\\nLine 2\r\n\
             DTSTART:{today}T140000\r\nDTEND:{today}T143000\r\n\
             END:VEVENT\r\n"
        );
        let events = parse_ics_events(&body, at_local(0, 0), at_local(23, 59));
        assert_eq!(events[0].title, "Hello, World\nLine 2");
    }

    #[test]
    fn ics_filters_events_outside_window() {
        // Event yesterday should not appear.
        let yesterday = (Local::now() - chrono::Duration::days(1))
            .format("%Y%m%d")
            .to_string();
        let body = format!(
            "BEGIN:VEVENT\r\nSUMMARY:Old\r\n\
             DTSTART:{yesterday}T140000\r\nDTEND:{yesterday}T143000\r\n\
             END:VEVENT\r\n"
        );
        let events = parse_ics_events(&body, at_local(0, 0), at_local(23, 59));
        assert!(events.is_empty());
    }

    #[test]
    fn ics_handles_no_dtend_with_30min_default() {
        let now = Local::now();
        let today = now.format("%Y%m%d").to_string();
        let body = format!(
            "BEGIN:VEVENT\r\nSUMMARY:Quick\r\nDTSTART:{today}T140000\r\nEND:VEVENT\r\n"
        );
        let events = parse_ics_events(&body, at_local(0, 0), at_local(23, 59));
        assert_eq!(events.len(), 1);
        let dur = events[0].end - events[0].start;
        assert_eq!(dur.num_minutes(), 30);
    }

    #[test]
    fn ics_handles_line_folding() {
        // RFC 5545: lines starting with space continue the previous line.
        let now = Local::now();
        let today = now.format("%Y%m%d").to_string();
        let body = format!(
            "BEGIN:VEVENT\r\nSUMMARY:Very long event title that is\r\n folded across two lines\r\n\
             DTSTART:{today}T140000\r\nDTEND:{today}T143000\r\nEND:VEVENT\r\n"
        );
        let events = parse_ics_events(&body, at_local(0, 0), at_local(23, 59));
        assert_eq!(events[0].title, "Very long event title that isfolded across two lines");
    }

    #[test]
    fn ics_skips_unknown_properties() {
        let now = Local::now();
        let today = now.format("%Y%m%d").to_string();
        let body = format!(
            "BEGIN:VEVENT\r\nSUMMARY:Meeting\r\nRRULE:FREQ=WEEKLY\r\n\
             ATTENDEE;CN=Bob:mailto:bob@example.com\r\n\
             DTSTART:{today}T140000\r\nDTEND:{today}T143000\r\nEND:VEVENT\r\n"
        );
        let events = parse_ics_events(&body, at_local(0, 0), at_local(23, 59));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].title, "Meeting");
    }
}

// --- v2.0.0 — stub `ek` module on non-macOS targets ------------------
// Lets the rest of the codebase reference `infra::calendar::ek::CalendarReader`
// without per-call-site cfg gates. On Windows/Linux the reader simply
// returns an error explaining the limitation. Full Outlook/WinRT/iCloud
// integration is deferred to v2.1+.
#[cfg(not(target_os = "macos"))]
pub mod ek {
    use super::{CalendarError, CalendarEvent};

    pub struct CalendarReader;

    impl CalendarReader {
        pub fn new() -> Self {
            Self
        }
        pub fn request_access(&self) -> Result<bool, CalendarError> {
            Ok(false)
        }
        pub fn events_today(&self) -> Result<Vec<CalendarEvent>, CalendarError> {
            Ok(Vec::new())
        }
    }
}
