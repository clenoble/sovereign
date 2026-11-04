//! v0.0.12, the time ruler's days follow the calendar: tests for the contract
//! `doc/plans/V0012-ruler-calendar-days.md` (D1-D6). Written before the code, by the test
//! writer; the code author makes them pass without editing them.
//!
//! # The seam (these tests do not compile until it exists)
//!
//! In `crates/sovereign-shell/src/canvas.rs` (module `crate::canvas`; private is fine, this file
//! is a child module declared there and reaches it through `super::*`):
//!
//! ```ignore
//! fn axis_ticks_in<Tz: TimeZone>(tz: &Tz, t0: i64, t1: i64, px_per_day: f64) -> (Vec<i64>, AxisFmt);
//! fn axis_label_in<Tz: TimeZone>(tz: &Tz, ts: i64, fmt: AxisFmt) -> String;
//! ```
//!
//! - `tz` is the zone every local reading is made in: alignment, midnights, wall-clock hours,
//!   months, years and the label text. Nothing inside reads `Local` except through `tz`.
//! - **Production calls them with `Local`.** `axis_ticks(t0, t1, px_per_day)` and
//!   `axis_label(ts, fmt)` keep their signatures and are exactly `axis_ticks_in(&Local, ..)` and
//!   `axis_label_in(&Local, ..)` (pinned below).
//! - The tests call the seam with three zones: `TestZone` (this file), `chrono::Local` and
//!   `chrono::FixedOffset`. All three are `Copy + Debug`, so a bound tighter than `TimeZone` that
//!   they meet is acceptable; the parameters and return types are not negotiable.
//! - `AxisFmt` is used as it is (no derive needed: the tests use `matches!`).
//!
//! # The test zones
//!
//! `chrono-tz` is not in the lock, so the 23- and 25-hour days come from a zone written here,
//! `TestZone`, with fixed rules: UTC+1, and UTC+2 between two instants. Its change dates are
//! deliberately **not** Zurich's (2026-03-29 and 2026-10-25), and its offsets are not UTC's, so an
//! implementation that reads `Local` instead of the zone it is given fails on PANDA2, the laptop
//! and CI alike.
//!
//! - `zone()`: spring 2026-03-08 01:00 UTC (local 02:00 -> 03:00, 2026-03-08 lasts 23 h), autumn
//!   2026-11-01 01:00 UTC (local 03:00 -> 02:00, 2026-11-01 lasts 25 h, 02:xx occurs twice).
//! - `midnight_zone()`: the change at local midnight. Spring 2026-04-04 23:00 UTC (local 00:00 ->
//!   01:00: 2026-04-05 has no 00:00), autumn 2026-10-03 23:00 UTC (local 01:00 -> 00:00:
//!   2026-10-04 has 00:00 twice). This is D1's "the earlier one if 00:00 occurs twice; if 00:00
//!   does not exist that day, the first instant of that date".
//!
//! # Picking an interval
//!
//! `axis_ticks` picks the smallest interval whose length is at least
//! `want = (76 / px_per_day) * 86400` seconds. `Iv::px()` asks for `want = 0.9 * interval` (and
//! 4000 days for the open-ended 10-year interval), which lands strictly inside the interval's
//! band; `the_px_per_day_used_here_selects_each_interval` checks the arithmetic, and every test
//! that relies on an interval also checks the returned `AxisFmt`.

use super::*;
use chrono::{
    DateTime, Datelike, Days, Duration, FixedOffset, Local, LocalResult, Months, NaiveDate, NaiveDateTime,
    Offset, TimeZone, Timelike,
};

// ---- the test zone ----------------------------------------------------------------------

/// Fixed rules: `std_secs` east of UTC, and `dst_secs` from `spring` (inclusive) to `autumn`
/// (exclusive), both UTC timestamps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Rules {
    std_secs: i32,
    dst_secs: i32,
    spring: i64,
    autumn: i64,
}

impl Rules {
    fn offset_secs_at(&self, utc_ts: i64) -> i32 {
        if utc_ts >= self.spring && utc_ts < self.autumn {
            self.dst_secs
        } else {
            self.std_secs
        }
    }
    fn offset(&self, secs: i32) -> TestOffset {
        TestOffset { fixed: FixedOffset::east_opt(secs).expect("offset in range"), rules: *self }
    }
}

/// The offset carries its rules, so `TimeZone::from_offset` gives back the same zone (chrono
/// rebuilds the zone from the offset after every arithmetic step).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TestOffset {
    fixed: FixedOffset,
    rules: Rules,
}

impl Offset for TestOffset {
    fn fix(&self) -> FixedOffset {
        self.fixed
    }
}

#[derive(Clone, Copy, Debug)]
struct TestZone(Rules);

impl TimeZone for TestZone {
    type Offset = TestOffset;

    fn from_offset(offset: &TestOffset) -> Self {
        TestZone(offset.rules)
    }

    fn offset_from_local_date(&self, local: &NaiveDate) -> LocalResult<TestOffset> {
        self.offset_from_local_datetime(&local.and_hms_opt(0, 0, 0).expect("midnight"))
    }

    /// A local time is valid under an offset when that offset is the one in force at the
    /// instant it designates. Two valid offsets: the repeated hour, `Ambiguous(earlier, later)`
    /// (the larger offset designates the earlier instant). None: the skipped hour.
    fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> LocalResult<TestOffset> {
        let r = self.0;
        let (hi, lo) = if r.dst_secs >= r.std_secs { (r.dst_secs, r.std_secs) } else { (r.std_secs, r.dst_secs) };
        let wall = local.and_utc().timestamp();
        let mut valid: Vec<i32> = Vec::new();
        for off in [hi, lo] {
            if r.offset_secs_at(wall - off as i64) == off && !valid.contains(&off) {
                valid.push(off);
            }
        }
        match valid.as_slice() {
            [] => LocalResult::None,
            [o] => LocalResult::Single(r.offset(*o)),
            [earlier, later] => LocalResult::Ambiguous(r.offset(*earlier), r.offset(*later)),
            _ => unreachable!(),
        }
    }

    fn offset_from_utc_date(&self, utc: &NaiveDate) -> TestOffset {
        self.offset_from_utc_datetime(&utc.and_hms_opt(0, 0, 0).expect("midnight"))
    }

    fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> TestOffset {
        let r = self.0;
        r.offset(r.offset_secs_at(utc.and_utc().timestamp()))
    }
}

/// A UTC instant, as a timestamp.
fn utc(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
    NaiveDate::from_ymd_opt(y, mo, d).unwrap().and_hms_opt(h, mi, 0).unwrap().and_utc().timestamp()
}

/// UTC+1, UTC+2 from 2026-03-08 01:00 UTC to 2026-11-01 01:00 UTC.
fn zone() -> TestZone {
    TestZone(Rules { std_secs: 3600, dst_secs: 7200, spring: utc(2026, 3, 8, 1, 0), autumn: utc(2026, 11, 1, 1, 0) })
}

/// UTC+1, UTC+2 from 2026-04-04 23:00 UTC to 2026-10-03 23:00 UTC: the changes fall on local
/// midnight (2026-04-05 has no 00:00, 2026-10-04 has it twice).
fn midnight_zone() -> TestZone {
    TestZone(Rules { std_secs: 3600, dst_secs: 7200, spring: utc(2026, 4, 4, 23, 0), autumn: utc(2026, 10, 3, 23, 0) })
}

/// Every change of both test zones, with a name for the messages.
fn changes() -> Vec<(&'static str, TestZone, i64)> {
    let (z, m) = (zone(), midnight_zone());
    vec![
        ("zone spring (23-hour 2026-03-08)", z, z.0.spring),
        ("zone autumn (25-hour 2026-11-01)", z, z.0.autumn),
        ("midnight_zone spring (no 00:00 on 2026-04-05)", m, m.0.spring),
        ("midnight_zone autumn (00:00 twice on 2026-10-04)", m, m.0.autumn),
    ]
}

fn dt_in<Tz: TimeZone>(tz: &Tz, ts: i64) -> DateTime<Tz> {
    tz.timestamp_opt(ts, 0).single().expect("a timestamp maps to one local time")
}

fn show<Tz: TimeZone>(tz: &Tz, ts: i64) -> String {
    let d = dt_in(tz, ts);
    format!("{ts} = {} ({})", d.naive_local(), d.offset().fix())
}

fn show_all<Tz: TimeZone>(tz: &Tz, ticks: &[i64]) -> String {
    ticks.iter().map(|&t| show(tz, t)).collect::<Vec<_>>().join(", ")
}

fn labels<Tz: TimeZone>(tz: &Tz, ticks: &[i64], fmt: AxisFmt) -> Vec<String> {
    ticks.iter().map(|&t| axis_label_in(tz, t, fmt)).collect()
}

/// The first instant whose local date is `d` (its 00:00, the earlier one if 00:00 occurs twice;
/// else the first local minute that exists).
fn first_instant<Tz: TimeZone>(tz: &Tz, d: NaiveDate) -> i64 {
    for min in 0..24 * 60 {
        let n = d.and_hms_opt(min / 60, min % 60, 0).unwrap();
        if let Some(x) = tz.from_local_datetime(&n).earliest() {
            return x.timestamp();
        }
    }
    panic!("no instant on {d}");
}

// ---- intervals --------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum Iv {
    Min(u32),
    Hour(u32),
    Day(u32),
    Month(u32),
    Year(i32),
}

const ALL: [Iv; 21] = [
    Iv::Min(1),
    Iv::Min(2),
    Iv::Min(5),
    Iv::Min(10),
    Iv::Min(15),
    Iv::Min(30),
    Iv::Hour(1),
    Iv::Hour(2),
    Iv::Hour(3),
    Iv::Hour(6),
    Iv::Hour(12),
    Iv::Day(1),
    Iv::Day(2),
    Iv::Day(7),
    Iv::Month(1),
    Iv::Month(3),
    Iv::Month(6),
    Iv::Year(1),
    Iv::Year(2),
    Iv::Year(5),
    Iv::Year(10),
];

const DAY: f64 = 86_400.0;

impl Iv {
    /// The upper bound of `want` that selects this interval in `axis_ticks` (today's table).
    fn upper(self) -> f64 {
        match self {
            Iv::Min(m) => m as f64 * 60.0,
            Iv::Hour(h) => h as f64 * 3600.0,
            Iv::Day(d) => d as f64 * DAY,
            Iv::Month(1) => 31.0 * DAY,
            Iv::Month(3) => 93.0 * DAY,
            Iv::Month(6) => 186.0 * DAY,
            Iv::Year(1) => 366.0 * DAY,
            Iv::Year(2) => 2.0 * 366.0 * DAY,
            Iv::Year(5) => 5.0 * 366.0 * DAY,
            Iv::Year(10) => f64::INFINITY,
            other => panic!("not an interval of the ruler: {other:?}"),
        }
    }
    /// The `want` this file asks for: inside the band (previous upper, upper].
    fn want(self) -> f64 {
        match self {
            Iv::Year(10) => 4000.0 * DAY,
            iv => 0.9 * iv.upper(),
        }
    }
    fn px(self) -> f64 {
        76.0 * DAY / self.want()
    }
    /// A nominal length, to size test ranges.
    fn nominal_secs(self) -> i64 {
        match self {
            Iv::Min(m) => m as i64 * 60,
            Iv::Hour(h) => h as i64 * 3600,
            Iv::Day(d) => d as i64 * 86_400,
            Iv::Month(n) => n as i64 * 31 * 86_400,
            Iv::Year(n) => n as i64 * 366 * 86_400,
        }
    }
    /// The format today's code gives this interval (D6).
    fn fmt_name(self) -> &'static str {
        match self {
            Iv::Min(_) | Iv::Hour(_) => "Hm",
            Iv::Day(_) => "Day",
            Iv::Month(_) => "Month",
            Iv::Year(_) => "Year",
        }
    }
}

fn fmt_name(f: AxisFmt) -> &'static str {
    match f {
        AxisFmt::Hm => "Hm",
        AxisFmt::Day => "Day",
        AxisFmt::Month => "Month",
        AxisFmt::Year => "Year",
    }
}

/// `axis_ticks_in` at interval `iv`, checking that the interval's format came back.
fn ticks_at<Tz: TimeZone>(tz: &Tz, iv: Iv, t0: i64, t1: i64) -> Vec<i64> {
    let (ticks, fmt) = axis_ticks_in(tz, t0, t1, iv.px());
    assert_eq!(fmt_name(fmt), iv.fmt_name(), "{iv:?} (px_per_day {}) returned the wrong format", iv.px());
    ticks
}

// ---- today's rule, written out (the reference for D3 and D5) ----------------------------

fn ref_fixed<Tz: TimeZone>(t0: i64, t1: i64, start: DateTime<Tz>, step: Duration) -> Vec<i64> {
    let mut out = Vec::new();
    let mut dt = start;
    let mut guard = 0;
    while dt.timestamp() <= t1 && guard < 2000 {
        if dt.timestamp() >= t0 {
            out.push(dt.timestamp());
        }
        dt += step;
        guard += 1;
    }
    out
}

fn ref_months<Tz: TimeZone>(tz: &Tz, t0: i64, t1: i64, step: u32) -> Vec<i64> {
    let base = dt_in(tz, t0);
    let mut dt = tz.with_ymd_and_hms(base.year(), base.month(), 1, 0, 0, 0).single().unwrap_or(base);
    let off = dt.month0() % step;
    if off != 0 {
        let back = dt.clone().checked_sub_months(Months::new(off));
        dt = back.unwrap_or(dt);
    }
    let mut out = Vec::new();
    let mut guard = 0;
    while dt.timestamp() <= t1 && guard < 2000 {
        if dt.timestamp() >= t0 {
            out.push(dt.timestamp());
        }
        dt = match dt.checked_add_months(Months::new(step)) {
            Some(x) => x,
            None => break,
        };
        guard += 1;
    }
    out
}

fn ref_years<Tz: TimeZone>(tz: &Tz, t0: i64, t1: i64, step: i32) -> Vec<i64> {
    let y0 = dt_in(tz, t0).year();
    let mut y = y0 - y0.rem_euclid(step);
    let mut out = Vec::new();
    let mut guard = 0;
    while guard < 2000 {
        let ts = match tz.with_ymd_and_hms(y, 1, 1, 0, 0, 0).single() {
            Some(d) => d.timestamp(),
            None => break,
        };
        if ts > t1 {
            break;
        }
        if ts >= t0 {
            out.push(ts);
        }
        y += step;
        guard += 1;
    }
    out
}

/// Today's `axis_ticks` at interval `iv`, in zone `tz`: fixed steps from the aligned start for
/// minutes, hours and days; calendar steps for months and years.
fn ref_ticks<Tz: TimeZone>(tz: &Tz, iv: Iv, t0: i64, t1: i64) -> Vec<i64> {
    let base = dt_in(tz, t0);
    let z = base.clone() - Duration::seconds(base.second() as i64) - Duration::nanoseconds(base.nanosecond() as i64);
    match iv {
        Iv::Min(m) => {
            let start = z.clone() - Duration::minutes((z.minute() % m) as i64);
            ref_fixed(t0, t1, start, Duration::minutes(m as i64))
        }
        Iv::Hour(h) => {
            let b = z.clone() - Duration::minutes(z.minute() as i64);
            let start = b.clone() - Duration::hours((b.hour() % h) as i64);
            ref_fixed(t0, t1, start, Duration::hours(h as i64))
        }
        Iv::Day(d) => {
            let midnight = base
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .and_then(|n| tz.from_local_datetime(&n).earliest())
                .unwrap_or(base.clone());
            ref_fixed(t0, t1, midnight, Duration::days(d as i64))
        }
        Iv::Month(n) => ref_months(tz, t0, t1, n),
        Iv::Year(n) => ref_years(tz, t0, t1, n),
    }
}

// ---- the contract's rules, written out (the reference for D1 and D2) ---------------------

/// D1: the first instant of the left edge's date, then of every `step`-th date after it, kept
/// when within `[t0, t1]`.
fn expected_day_ticks<Tz: TimeZone>(tz: &Tz, t0: i64, t1: i64, step: u64) -> Vec<i64> {
    let mut d = dt_in(tz, t0).date_naive();
    let mut out = Vec::new();
    loop {
        let inst = first_instant(tz, d);
        if inst > t1 {
            break;
        }
        if inst >= t0 {
            out.push(inst);
        }
        d = d.checked_add_days(Days::new(step)).unwrap();
    }
    out
}

/// D2: every instant whose local time is `h:00` with `h` a multiple of `step`, date by date;
/// the earlier one when `h:00` occurs twice, none when it does not exist.
fn expected_hour_ticks<Tz: TimeZone>(tz: &Tz, t0: i64, t1: i64, step: u32) -> Vec<i64> {
    let first = dt_in(tz, t0).date_naive().pred_opt().unwrap();
    let last = dt_in(tz, t1).date_naive().succ_opt().unwrap();
    let mut out = Vec::new();
    let mut d = first;
    while d <= last {
        for h in (0..24).step_by(step as usize) {
            let n = d.and_hms_opt(h, 0, 0).unwrap();
            if let Some(x) = tz.from_local_datetime(&n).earliest() {
                let ts = x.timestamp();
                if ts >= t0 && ts <= t1 {
                    out.push(ts);
                }
            }
        }
        d = d.succ_opt().unwrap();
    }
    out.sort_unstable();
    out
}

fn assert_well_formed<Tz: TimeZone>(tz: &Tz, iv: Iv, t0: i64, t1: i64, ticks: &[i64], what: &str) {
    assert!(
        ticks.windows(2).all(|w| w[0] < w[1]),
        "{what}: {iv:?} ticks not strictly increasing from t0 {}: {}",
        show(tz, t0),
        show_all(tz, ticks)
    );
    assert!(
        ticks.iter().all(|&t| t >= t0 && t <= t1),
        "{what}: {iv:?} a tick outside [{}, {}]: {}",
        show(tz, t0),
        show(tz, t1),
        show_all(tz, ticks)
    );
}

// ---- the fixtures themselves ------------------------------------------------------------

/// If the test zones were wrong, every test below would test nothing: check them first.
#[test]
fn ruler_days_the_test_zones_have_a_23_and_a_25_hour_day() {
    let z = zone();
    let day_len = |tz: &TestZone, y, m, d| {
        let a = NaiveDate::from_ymd_opt(y, m, d).unwrap();
        first_instant(tz, a.succ_opt().unwrap()) - first_instant(tz, a)
    };
    assert_eq!(day_len(&z, 2026, 3, 7), 24 * 3600);
    assert_eq!(day_len(&z, 2026, 3, 8), 23 * 3600);
    assert_eq!(day_len(&z, 2026, 3, 9), 24 * 3600);
    assert_eq!(day_len(&z, 2026, 10, 31), 24 * 3600);
    assert_eq!(day_len(&z, 2026, 11, 1), 25 * 3600);
    assert_eq!(day_len(&z, 2026, 11, 2), 24 * 3600);

    let at = |tz: &TestZone, m, d, h, mi| {
        tz.from_local_datetime(&NaiveDate::from_ymd_opt(2026, m, d).unwrap().and_hms_opt(h, mi, 0).unwrap())
    };
    // The repeated hour: two instants, the earlier first, an hour apart.
    match at(&z, 11, 1, 2, 30) {
        LocalResult::Ambiguous(a, b) => {
            assert_eq!(a.timestamp(), utc(2026, 11, 1, 0, 30));
            assert_eq!(b.timestamp(), utc(2026, 11, 1, 1, 30));
        }
        other => panic!("02:30 on 2026-11-01 should be ambiguous: {other:?}"),
    }
    // The skipped hour: nothing.
    assert!(matches!(at(&z, 3, 8, 2, 30), LocalResult::None));
    assert!(matches!(at(&z, 3, 8, 3, 0), LocalResult::Single(_)));
    assert!(matches!(at(&z, 6, 1, 2, 30), LocalResult::Single(_)));
    // Readings from UTC.
    assert_eq!(dt_in(&z, utc(2026, 11, 1, 0, 0)).hour(), 2);
    assert_eq!(dt_in(&z, utc(2026, 11, 1, 1, 0)).hour(), 2);
    assert_eq!(dt_in(&z, utc(2026, 3, 8, 1, 0)).hour(), 3);
    // Arithmetic keeps the rules (chrono rebuilds the zone from the offset).
    let d = dt_in(&z, utc(2026, 10, 31, 22, 0)) + Duration::hours(24);
    assert_eq!((d.hour(), d.day()), (23, 1));

    let m = midnight_zone();
    assert_eq!(day_len(&m, 2026, 4, 5), 23 * 3600);
    assert_eq!(day_len(&m, 2026, 10, 4), 25 * 3600);
    assert!(matches!(at(&m, 4, 5, 0, 0), LocalResult::None));
    assert_eq!(first_instant(&m, NaiveDate::from_ymd_opt(2026, 4, 5).unwrap()), utc(2026, 4, 4, 23, 0));
    match at(&m, 10, 4, 0, 0) {
        LocalResult::Ambiguous(a, b) => {
            assert_eq!(a.timestamp(), utc(2026, 10, 3, 22, 0));
            assert_eq!(b.timestamp(), utc(2026, 10, 3, 23, 0));
        }
        other => panic!("00:00 on 2026-10-04 should be ambiguous: {other:?}"),
    }
}

/// The arithmetic of `Iv::px`: each `px_per_day` used here gives a `want` inside its interval's
/// band, so `axis_ticks` picks that interval (and not a neighbour).
#[test]
fn ruler_days_the_px_per_day_used_here_selects_each_interval() {
    let mut prev = 0.0;
    for iv in ALL {
        let want = (76.0 / iv.px().max(1e-6)) * DAY; // the formula in `axis_ticks`
        assert!(want > prev && want <= iv.upper(), "{iv:?}: want {want} not in ({prev}, {}]", iv.upper());
        prev = iv.upper();
    }
}

// ---- the seam ---------------------------------------------------------------------------

/// The seam reads the zone it is given, not the machine's. +05:30 is no machine's zone here
/// (PANDA2 and the laptop are Europe/Zurich, CI is UTC).
#[test]
fn ruler_days_the_seam_uses_the_zone_it_is_given() {
    let ist = FixedOffset::east_opt(5 * 3600 + 1800).unwrap();
    let t0 = utc(2026, 6, 10, 7, 13);
    let days = ticks_at(&ist, Iv::Day(1), t0, t0 + 5 * 86_400);
    assert_eq!(days.len(), 5, "{}", show_all(&ist, &days));
    // Local midnight at +05:30 is 18:30 UTC.
    assert!(days.iter().all(|&t| t.rem_euclid(86_400) == 18 * 3600 + 1800), "{}", show_all(&ist, &days));
    let six = ticks_at(&ist, Iv::Hour(6), t0, t0 + 86_400);
    assert_eq!(six.len(), 4, "{}", show_all(&ist, &six));
    assert!(six.iter().all(|&t| t.rem_euclid(6 * 3600) == 1800), "{}", show_all(&ist, &six));
    assert_eq!(axis_label_in(&ist, utc(2026, 6, 10, 18, 30), AxisFmt::Day), "Jun 11");
    assert_eq!(axis_label_in(&ist, utc(2026, 6, 10, 18, 30), AxisFmt::Hm), "00:00");
    // And the test zone is read too: 22:00 UTC on 2026-10-31 is Nov 1, 00:00 there.
    assert_eq!(axis_label_in(&zone(), utc(2026, 10, 31, 22, 0), AxisFmt::Day), "Nov 1");
}

/// `axis_ticks` and `axis_label` keep their signatures and are the seam called with `Local`.
#[test]
fn ruler_days_axis_ticks_and_axis_label_are_the_seam_with_local() {
    let t0s = [utc(2026, 1, 15, 9, 41), utc(2026, 3, 28, 23, 7), utc(2026, 10, 24, 22, 59), utc(2026, 7, 1, 0, 0)];
    for t0 in t0s {
        for iv in ALL {
            let t1 = t0 + 6 * iv.nominal_secs() + 1234;
            let (a, fa) = axis_ticks(t0, t1, iv.px());
            let (b, fb) = axis_ticks_in(&Local, t0, t1, iv.px());
            assert_eq!(a, b, "{iv:?} at t0 {t0}: axis_ticks differs from axis_ticks_in(&Local, ..)");
            assert_eq!(fmt_name(fa), fmt_name(fb));
            for &t in a.iter().take(5) {
                for f in [AxisFmt::Hm, AxisFmt::Day, AxisFmt::Month, AxisFmt::Year] {
                    assert_eq!(axis_label(t, f), axis_label_in(&Local, t, f));
                }
            }
        }
    }
}

// ---- D1: day ticks are calendar dates ---------------------------------------------------

/// D1 at the 1-day interval over the 25-hour day: one tick per date, at its local 00:00, each
/// date labelled once (today: `Nov 1` twice, the second at 23:00).
#[test]
fn ruler_days_d1_one_day_ticks_label_each_date_once_across_the_25_hour_day() {
    let z = zone();
    let ticks = ticks_at(&z, Iv::Day(1), utc(2026, 10, 29, 10, 0), utc(2026, 11, 5, 10, 0));
    let expected = vec![
        utc(2026, 10, 29, 22, 0), // Oct 30 00:00 (+2)
        utc(2026, 10, 30, 22, 0), // Oct 31 00:00 (+2)
        utc(2026, 10, 31, 22, 0), // Nov 1 00:00 (+2), a 25-hour day
        utc(2026, 11, 1, 23, 0),  // Nov 2 00:00 (+1)
        utc(2026, 11, 2, 23, 0),
        utc(2026, 11, 3, 23, 0),
        utc(2026, 11, 4, 23, 0), // Nov 5 00:00 (+1)
    ];
    assert_eq!(ticks, expected, "got {}", show_all(&z, &ticks));
    assert_eq!(
        labels(&z, &ticks, AxisFmt::Day),
        ["Oct 30", "Oct 31", "Nov 1", "Nov 2", "Nov 3", "Nov 4", "Nov 5"]
    );
}

/// D1 at the 1-day interval over the 23-hour day (today: from Mar 9 on, every tick at 01:00).
#[test]
fn ruler_days_d1_one_day_ticks_label_each_date_once_across_the_23_hour_day() {
    let z = zone();
    let ticks = ticks_at(&z, Iv::Day(1), utc(2026, 3, 5, 10, 0), utc(2026, 3, 12, 10, 0));
    let expected = vec![
        utc(2026, 3, 5, 23, 0), // Mar 6 00:00 (+1)
        utc(2026, 3, 6, 23, 0),
        utc(2026, 3, 7, 23, 0), // Mar 8 00:00 (+1), a 23-hour day
        utc(2026, 3, 8, 22, 0), // Mar 9 00:00 (+2)
        utc(2026, 3, 9, 22, 0),
        utc(2026, 3, 10, 22, 0),
        utc(2026, 3, 11, 22, 0), // Mar 12 00:00 (+2)
    ];
    assert_eq!(ticks, expected, "got {}", show_all(&z, &ticks));
    assert_eq!(
        labels(&z, &ticks, AxisFmt::Day),
        ["Mar 6", "Mar 7", "Mar 8", "Mar 9", "Mar 10", "Mar 11", "Mar 12"]
    );
}

/// D1 at the 2- and 7-day intervals across both changes: 2 or 7 calendar dates apart, from the
/// left edge's date, each at 00:00.
#[test]
fn ruler_days_d1_two_and_seven_day_ticks_step_by_calendar_dates() {
    let z = zone();
    let cases: [(Iv, i64, i64, Vec<i64>, Vec<&str>); 4] = [
        (
            Iv::Day(2),
            utc(2026, 10, 27, 10, 0), // Oct 27, 12:00 local
            utc(2026, 11, 7, 10, 0),
            vec![
                utc(2026, 10, 28, 22, 0),
                utc(2026, 10, 30, 22, 0),
                utc(2026, 11, 1, 23, 0),
                utc(2026, 11, 3, 23, 0),
                utc(2026, 11, 5, 23, 0),
            ],
            vec!["Oct 29", "Oct 31", "Nov 2", "Nov 4", "Nov 6"],
        ),
        (
            Iv::Day(2),
            utc(2026, 3, 4, 10, 0), // Mar 4, 11:00 local
            utc(2026, 3, 13, 0, 0),
            vec![utc(2026, 3, 5, 23, 0), utc(2026, 3, 7, 23, 0), utc(2026, 3, 9, 22, 0), utc(2026, 3, 11, 22, 0)],
            vec!["Mar 6", "Mar 8", "Mar 10", "Mar 12"],
        ),
        (
            Iv::Day(7),
            utc(2026, 10, 20, 10, 0), // Oct 20, 12:00 local
            utc(2026, 11, 18, 0, 0),
            vec![utc(2026, 10, 26, 22, 0), utc(2026, 11, 2, 23, 0), utc(2026, 11, 9, 23, 0), utc(2026, 11, 16, 23, 0)],
            vec!["Oct 27", "Nov 3", "Nov 10", "Nov 17"],
        ),
        (
            Iv::Day(7),
            utc(2026, 3, 2, 10, 0), // Mar 2, 11:00 local
            utc(2026, 3, 24, 0, 0),
            vec![utc(2026, 3, 8, 22, 0), utc(2026, 3, 15, 22, 0), utc(2026, 3, 22, 22, 0)],
            vec!["Mar 9", "Mar 16", "Mar 23"],
        ),
    ];
    for (iv, t0, t1, expected, names) in cases {
        let ticks = ticks_at(&z, iv, t0, t1);
        assert_eq!(ticks, expected, "{iv:?} from {}: got {}", show(&z, t0), show_all(&z, &ticks));
        assert_eq!(labels(&z, &ticks, AxisFmt::Day), names, "{iv:?} from {}", show(&z, t0));
    }
}

/// D1's two edge cases: when 00:00 occurs twice, the earlier; when it does not exist, the first
/// instant of the date.
#[test]
fn ruler_days_d1_a_repeated_midnight_ticks_the_earlier_and_a_missing_one_the_first_instant() {
    let m = midnight_zone();
    let ticks = ticks_at(&m, Iv::Day(1), utc(2026, 4, 2, 12, 0), utc(2026, 4, 8, 12, 0));
    let expected = vec![
        utc(2026, 4, 2, 23, 0), // Apr 3 00:00 (+1)
        utc(2026, 4, 3, 23, 0), // Apr 4 00:00 (+1)
        utc(2026, 4, 4, 23, 0), // Apr 5 has no 00:00: its first instant, 01:00 (+2)
        utc(2026, 4, 5, 22, 0), // Apr 6 00:00 (+2)
        utc(2026, 4, 6, 22, 0),
        utc(2026, 4, 7, 22, 0),
    ];
    assert_eq!(ticks, expected, "got {}", show_all(&m, &ticks));
    assert_eq!(labels(&m, &ticks, AxisFmt::Day), ["Apr 3", "Apr 4", "Apr 5", "Apr 6", "Apr 7", "Apr 8"]);

    let ticks = ticks_at(&m, Iv::Day(1), utc(2026, 10, 1, 12, 0), utc(2026, 10, 7, 12, 0));
    let expected = vec![
        utc(2026, 10, 1, 22, 0), // Oct 2 00:00 (+2)
        utc(2026, 10, 2, 22, 0), // Oct 3 00:00 (+2)
        utc(2026, 10, 3, 22, 0), // Oct 4 00:00, the earlier of two (+2)
        utc(2026, 10, 4, 23, 0), // Oct 5 00:00 (+1)
        utc(2026, 10, 5, 23, 0),
        utc(2026, 10, 6, 23, 0),
    ];
    assert_eq!(ticks, expected, "got {}", show_all(&m, &ticks));
    assert_eq!(labels(&m, &ticks, AxisFmt::Day), ["Oct 2", "Oct 3", "Oct 4", "Oct 5", "Oct 6", "Oct 7"]);
}

/// D1 as a property, near every change of both zones and for many left edges: at 1, 2 and 7
/// days the ticks are exactly the first instants of the left edge's date and of every 1st, 2nd
/// or 7th date after it; so at 1 day every date in range is labelled exactly once.
#[test]
fn ruler_days_d1_day_ticks_are_the_dates_first_instants_near_every_change() {
    let mut checked = 0usize;
    for (name, z, change) in changes() {
        for (iv, step) in [(Iv::Day(1), 1u64), (Iv::Day(2), 2), (Iv::Day(7), 7)] {
            let span = 5 * step as i64 * 86_400 + 3 * 3600;
            let mut t0 = change - span - 86_400;
            while t0 <= change + 86_400 {
                let t1 = t0 + span;
                let ticks = ticks_at(&z, iv, t0, t1);
                let expected = expected_day_ticks(&z, t0, t1, step);
                assert_eq!(
                    ticks,
                    expected,
                    "{name}, {iv:?} from {} to {}:\n got      {}\n expected {}",
                    show(&z, t0),
                    show(&z, t1),
                    show_all(&z, &ticks),
                    show_all(&z, &expected)
                );
                if step == 1 {
                    let l = labels(&z, &ticks, AxisFmt::Day);
                    let mut u = l.clone();
                    u.sort();
                    u.dedup();
                    assert_eq!(u.len(), l.len(), "{name}: a date labelled twice: {l:?}");
                    let dates: Vec<NaiveDate> = ticks.iter().map(|&t| dt_in(&z, t).date_naive()).collect();
                    assert!(
                        dates.windows(2).all(|w| w[0].succ_opt() == Some(w[1])),
                        "{name}: dates not consecutive: {dates:?}"
                    );
                }
                checked += 1;
                t0 += 97 * 60; // 1 h 37 min: every hour and minute position in turn
            }
        }
    }
    eprintln!("D1 property: {checked} left edges checked");
}

// ---- D2: multi-hour ticks are wall-clock hours ------------------------------------------

/// D2: 6-hour ticks read 00, 06, 12, 18 on both sides of each change (today: 05, 11, 17 after
/// the autumn change, 07, 13, 19 after the spring one).
#[test]
fn ruler_days_d2_six_hour_ticks_read_00_06_12_18_on_both_sides_of_each_change() {
    let z = zone();
    let names = ["06:00", "12:00", "18:00", "00:00", "06:00", "12:00", "18:00", "00:00"];

    let ticks = ticks_at(&z, Iv::Hour(6), utc(2026, 10, 31, 0, 0), utc(2026, 11, 2, 0, 0));
    let expected = vec![
        utc(2026, 10, 31, 4, 0), // Oct 31 06:00 (+2)
        utc(2026, 10, 31, 10, 0),
        utc(2026, 10, 31, 16, 0),
        utc(2026, 10, 31, 22, 0), // Nov 1 00:00 (+2)
        utc(2026, 11, 1, 5, 0),   // Nov 1 06:00 (+1)
        utc(2026, 11, 1, 11, 0),
        utc(2026, 11, 1, 17, 0),
        utc(2026, 11, 1, 23, 0), // Nov 2 00:00 (+1)
    ];
    assert_eq!(ticks, expected, "autumn: got {}", show_all(&z, &ticks));
    assert_eq!(labels(&z, &ticks, AxisFmt::Hm), names);

    let ticks = ticks_at(&z, Iv::Hour(6), utc(2026, 3, 7, 0, 0), utc(2026, 3, 9, 0, 0));
    let expected = vec![
        utc(2026, 3, 7, 5, 0), // Mar 7 06:00 (+1)
        utc(2026, 3, 7, 11, 0),
        utc(2026, 3, 7, 17, 0),
        utc(2026, 3, 7, 23, 0), // Mar 8 00:00 (+1)
        utc(2026, 3, 8, 4, 0),  // Mar 8 06:00 (+2)
        utc(2026, 3, 8, 10, 0),
        utc(2026, 3, 8, 16, 0),
        utc(2026, 3, 8, 22, 0), // Mar 9 00:00 (+2)
    ];
    assert_eq!(ticks, expected, "spring: got {}", show_all(&z, &ticks));
    assert_eq!(labels(&z, &ticks, AxisFmt::Hm), names);
}

/// D2's rule for a change inside the interval, at 2 hours: in the repeated hour only the earlier
/// 02:00 is a tick; the skipped 02:00 has none.
#[test]
fn ruler_days_d2_two_hour_ticks_keep_the_earlier_repeated_hour_and_skip_the_missing_one() {
    let z = zone();

    let ticks = ticks_at(&z, Iv::Hour(2), utc(2026, 10, 31, 21, 30), utc(2026, 11, 1, 5, 30));
    let expected = vec![
        utc(2026, 10, 31, 22, 0), // 00:00 (+2)
        utc(2026, 11, 1, 0, 0),   // 02:00 (+2), the earlier
        utc(2026, 11, 1, 3, 0),   // 04:00 (+1)
        utc(2026, 11, 1, 5, 0),   // 06:00 (+1)
    ];
    assert_eq!(ticks, expected, "autumn: got {}", show_all(&z, &ticks));
    assert!(!ticks.contains(&utc(2026, 11, 1, 1, 0)), "the later 02:00 is not a tick");
    assert_eq!(labels(&z, &ticks, AxisFmt::Hm), ["00:00", "02:00", "04:00", "06:00"]);

    let ticks = ticks_at(&z, Iv::Hour(2), utc(2026, 3, 7, 22, 30), utc(2026, 3, 8, 4, 30));
    let expected = vec![
        utc(2026, 3, 7, 23, 0), // 00:00 (+1)
        // 02:00 does not exist on Mar 8: no tick
        utc(2026, 3, 8, 2, 0), // 04:00 (+2)
        utc(2026, 3, 8, 4, 0), // 06:00 (+2)
    ];
    assert_eq!(ticks, expected, "spring: got {}", show_all(&z, &ticks));
    assert_eq!(labels(&z, &ticks, AxisFmt::Hm), ["00:00", "04:00", "06:00"]);
}

/// D2 as a property, near every change of both zones (the midnight zone puts the repeated and
/// the skipped hour on 00:00, a multiple of every interval): at 2, 3, 6 and 12 hours the ticks
/// are exactly the wall-clock `h:00` instants of the rule.
#[test]
fn ruler_days_d2_multi_hour_ticks_are_wall_clock_hours_near_every_change() {
    let mut checked = 0usize;
    for (name, z, change) in changes() {
        for h in [2u32, 3, 6, 12] {
            let iv = Iv::Hour(h);
            let span = 8 * h as i64 * 3600 + 1000;
            let mut t0 = change - span - 2 * 86_400;
            while t0 <= change + 2 * 86_400 {
                let t1 = t0 + span;
                let ticks = ticks_at(&z, iv, t0, t1);
                let expected = expected_hour_ticks(&z, t0, t1, h);
                assert_eq!(
                    ticks,
                    expected,
                    "{name}, {iv:?} from {} to {}:\n got      {}\n expected {}",
                    show(&z, t0),
                    show(&z, t1),
                    show_all(&z, &ticks),
                    show_all(&z, &expected)
                );
                checked += 1;
                t0 += 47 * 60;
            }
        }
    }
    eprintln!("D2 property: {checked} left edges checked");
}

// ---- D3: minute and 1-hour ticks are unchanged -------------------------------------------

/// D3: at the autumn change the 1-hour ticks show 02:00 twice, two real instants an hour apart;
/// at the spring change they go 01:00, 03:00.
#[test]
fn ruler_days_d3_one_hour_ticks_show_both_0200_instants_at_the_autumn_change() {
    let z = zone();
    let ticks = ticks_at(&z, Iv::Hour(1), utc(2026, 10, 31, 22, 30), utc(2026, 11, 1, 4, 30));
    let expected: Vec<i64> = (0..6).map(|k| utc(2026, 10, 31, 23, 0) + k * 3600).collect();
    assert_eq!(ticks, expected, "autumn: got {}", show_all(&z, &ticks));
    assert!(ticks.contains(&utc(2026, 11, 1, 0, 0)) && ticks.contains(&utc(2026, 11, 1, 1, 0)));
    assert_eq!(utc(2026, 11, 1, 1, 0) - utc(2026, 11, 1, 0, 0), 3600);
    assert_eq!(labels(&z, &ticks, AxisFmt::Hm), ["01:00", "02:00", "02:00", "03:00", "04:00", "05:00"]);

    let ticks = ticks_at(&z, Iv::Hour(1), utc(2026, 3, 7, 22, 30), utc(2026, 3, 8, 3, 30));
    let expected: Vec<i64> = (0..5).map(|k| utc(2026, 3, 7, 23, 0) + k * 3600).collect();
    assert_eq!(ticks, expected, "spring: got {}", show_all(&z, &ticks));
    assert_eq!(labels(&z, &ticks, AxisFmt::Hm), ["00:00", "01:00", "03:00", "04:00", "05:00"]);
}

/// D3 as a property: near every change of both zones, the minute intervals and the 1-hour one
/// give exactly today's instants (fixed steps from the aligned start).
#[test]
fn ruler_days_d3_minute_and_one_hour_ticks_are_todays_fixed_steps_across_changes() {
    let mut checked = 0usize;
    for (name, z, change) in changes() {
        for iv in [Iv::Min(1), Iv::Min(2), Iv::Min(5), Iv::Min(10), Iv::Min(15), Iv::Min(30), Iv::Hour(1)] {
            let span = 10 * iv.nominal_secs() + 77;
            let mut t0 = change - span - 3 * 3600;
            while t0 <= change + 3 * 3600 {
                let t1 = t0 + span;
                let ticks = ticks_at(&z, iv, t0, t1);
                let expected = ref_ticks(&z, iv, t0, t1);
                assert_eq!(
                    ticks,
                    expected,
                    "{name}, {iv:?} from {}:\n got      {}\n expected {}",
                    show(&z, t0),
                    show_all(&z, &ticks),
                    show_all(&z, &expected)
                );
                checked += 1;
                t0 += 7 * 60 + 13;
            }
        }
    }
    eprintln!("D3 property: {checked} left edges checked");
}

// ---- D4: always well formed -------------------------------------------------------------

/// D4: at every interval, in both test zones, over a walk of 2026 (every 3 h 06 min 37 s, so
/// the left edge visits every minute position) and densely (every 5 min 17 s) for a day on
/// either side of each change: strictly increasing, within `[t0, t1]`, no panic.
#[test]
fn ruler_days_d4_ticks_are_increasing_and_in_range_at_every_interval_in_the_test_zones() {
    let mut calls = 0usize;
    for z in [zone(), midnight_zone()] {
        let mut t0 = utc(2025, 12, 31, 0, 0);
        while t0 < utc(2027, 1, 1, 0, 0) {
            for iv in ALL {
                let t1 = t0 + 6 * iv.nominal_secs() + 1234;
                let ticks = ticks_at(&z, iv, t0, t1);
                assert_well_formed(&z, iv, t0, t1, &ticks, "year walk");
                calls += 1;
            }
            t0 += 3 * 3600 + 397;
        }
    }
    for (name, z, change) in changes() {
        let mut t0 = change - 86_400;
        while t0 <= change + 86_400 {
            for iv in ALL {
                let t1 = t0 + 6 * iv.nominal_secs() + 1234;
                let ticks = ticks_at(&z, iv, t0, t1);
                assert_well_formed(&z, iv, t0, t1, &ticks, name);
                calls += 1;
            }
            t0 += 5 * 60 + 17;
        }
    }
    eprintln!("D4: {calls} calls checked");
}

// ---- D5: ordinary days unchanged --------------------------------------------------------

/// True when `tz`'s offset is the same at every hour of `[t0, t1]` (no change in range).
fn no_change_in<Tz: TimeZone>(tz: &Tz, t0: i64, t1: i64) -> bool {
    let first = dt_in(tz, t0).offset().fix();
    (t0..=t1).step_by(3600).all(|t| dt_in(tz, t).offset().fix() == first) && dt_in(tz, t1).offset().fix() == first
}

fn d5_check<Tz: TimeZone>(tz: &Tz, zone_name: &str) -> usize {
    // Unaligned left edges, 10 days each; the first crosses a month, the last a year.
    let t0s = [utc(2026, 6, 25, 5, 17) + 42, utc(2026, 1, 27, 13, 41) + 9, utc(2026, 12, 27, 3, 3) + 33];
    let mut n = 0;
    for t0 in t0s {
        let t1 = t0 + 10 * 86_400;
        if !no_change_in(tz, t0, t1) {
            eprintln!("D5: {zone_name} changes offset within {}..{}: range skipped", show(tz, t0), show(tz, t1));
            continue;
        }
        for iv in ALL {
            let ticks = ticks_at(tz, iv, t0, t1);
            let expected = ref_ticks(tz, iv, t0, t1);
            assert_eq!(
                ticks,
                expected,
                "{zone_name}, {iv:?} from {}:\n got      {}\n expected {}",
                show(tz, t0),
                show_all(tz, &ticks),
                show_all(tz, &expected)
            );
            // A wrong interval would also change the spacing: say so plainly.
            if let Iv::Min(_) | Iv::Hour(_) | Iv::Day(_) = iv {
                assert!(
                    ticks.windows(2).all(|w| w[1] - w[0] == iv.nominal_secs()),
                    "{zone_name}, {iv:?}: spacing is not {} s on an ordinary range",
                    iv.nominal_secs()
                );
            }
            n += 1;
        }
    }
    n
}

/// D5: on 10-day ranges with no change, every interval gives the same instants as today's rule,
/// written out above (`ref_ticks`), in the test zones, in a fixed offset and in `Local`.
#[test]
fn ruler_days_d5_on_ranges_without_a_change_every_interval_gives_todays_instants() {
    let z = zone();
    let m = midnight_zone();
    let ist = FixedOffset::east_opt(5 * 3600 + 1800).unwrap();
    // The test zones have no change in these ranges by construction.
    let mut n = d5_check(&z, "zone");
    n += d5_check(&m, "midnight_zone");
    n += d5_check(&ist, "+05:30");
    n += d5_check(&Local, "Local");
    assert!(n >= 3 * 3 * ALL.len(), "D5 compared only {n} (zone, range, interval) cases");
    eprintln!("D5: {n} (zone, range, interval) cases compared");
}

// ---- D6: labels and formats unchanged ---------------------------------------------------

/// D6: the format per interval is today's (Hm for minutes and hours, Day for 1, 2 and 7 days,
/// Month for 1, 3 and 6 months, Year for years), in the test zone and through `axis_ticks`.
#[test]
fn ruler_days_d6_the_format_per_interval_is_unchanged() {
    let z = zone();
    for t0 in [utc(2026, 6, 10, 7, 13), utc(2026, 10, 31, 23, 30), utc(2026, 3, 8, 0, 30)] {
        for iv in ALL {
            let t1 = t0 + 3 * iv.nominal_secs();
            let (_, f) = axis_ticks_in(&z, t0, t1, iv.px());
            assert_eq!(fmt_name(f), iv.fmt_name(), "{iv:?} in the test zone");
            let (_, f) = axis_ticks(t0, t1, iv.px());
            assert_eq!(fmt_name(f), iv.fmt_name(), "{iv:?} through axis_ticks");
        }
    }
}

/// D6: the label text is today's, read in the zone given.
#[test]
fn ruler_days_d6_labels_are_unchanged() {
    let z = zone();
    let cases: [(i64, AxisFmt, &str); 12] = [
        (utc(2026, 11, 1, 0, 0), AxisFmt::Hm, "02:00"),
        (utc(2026, 11, 1, 1, 0), AxisFmt::Hm, "02:00"),
        (utc(2026, 11, 1, 1, 45), AxisFmt::Hm, "02:45"),
        (utc(2026, 3, 8, 1, 5), AxisFmt::Hm, "03:05"),
        (utc(2026, 10, 31, 22, 0), AxisFmt::Day, "Nov 1"),
        (utc(2026, 3, 9, 5, 0), AxisFmt::Day, "Mar 9"),
        (utc(2026, 10, 31, 22, 0), AxisFmt::Month, "Nov 26"),
        (utc(2026, 8, 31, 22, 0), AxisFmt::Month, "Sep 26"),
        (utc(2026, 6, 1, 0, 0), AxisFmt::Year, "2026"),
        // 23:30 UTC on Dec 31 is already 2027 at +1: the zone is read.
        (utc(2026, 12, 31, 23, 30), AxisFmt::Year, "2027"),
        (utc(2026, 12, 31, 23, 30), AxisFmt::Day, "Jan 1"),
        (utc(2026, 12, 31, 23, 30), AxisFmt::Month, "Jan 27"),
    ];
    for (ts, f, want) in cases {
        assert_eq!(axis_label_in(&z, ts, f), want, "label of {} as {}", show(&z, ts), fmt_name(f));
    }
}
