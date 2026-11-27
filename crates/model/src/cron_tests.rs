use super::Cron;

/// 2026-11-27 00:00 UTC, a Friday.
const FRIDAY: u64 = 1_795_737_600_000;
const MINUTE: u64 = 60_000;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;

fn cron(text: &str) -> Cron {
    Cron::parse(text).unwrap()
}

#[test]
fn the_midnight_of_the_fixture_is_a_friday() {
    let date = crate::Date::from_unix_days(i64::try_from(FRIDAY / DAY).unwrap()).unwrap();
    assert_eq!(date.to_string(), "2026-11-27");
    assert_eq!((FRIDAY / DAY + 4) % 7, 5);
}

#[test]
fn every_minute_fires_at_the_next_minute() {
    assert_eq!(cron("* * * * *").next_after(FRIDAY), Some(FRIDAY + MINUTE));
    assert_eq!(cron("* * * * *").next_after(FRIDAY + 30_000), Some(FRIDAY + MINUTE));
}

#[test]
fn a_daily_time_fires_today_or_tomorrow() {
    let nine = cron("0 9 * * *");
    assert_eq!(nine.next_after(FRIDAY), Some(FRIDAY + 9 * HOUR));
    assert_eq!(nine.next_after(FRIDAY + 9 * HOUR), Some(FRIDAY + DAY + 9 * HOUR));
}

#[test]
fn steps_ranges_and_lists_match_their_values() {
    let every_fifteen = cron("*/15 * * * *");
    assert_eq!(every_fifteen.next_after(FRIDAY + MINUTE), Some(FRIDAY + 15 * MINUTE));
    let office = cron("30 8-17/3 * * 1-5");
    assert_eq!(office.next_after(FRIDAY), Some(FRIDAY + 8 * HOUR + 30 * MINUTE));
    assert_eq!(office.next_after(FRIDAY + 18 * HOUR), Some(FRIDAY + 3 * DAY + 8 * HOUR + 30 * MINUTE));
    let listed = cron("5,10 0 * * *");
    assert_eq!(listed.next_after(FRIDAY + 5 * MINUTE), Some(FRIDAY + 10 * MINUTE));
}

#[test]
fn sunday_is_zero_or_seven() {
    let sunday = FRIDAY + 2 * DAY;
    assert_eq!(cron("0 0 * * 0").next_after(FRIDAY), Some(sunday));
    assert_eq!(cron("0 0 * * 7").next_after(FRIDAY), Some(sunday));
    assert_eq!(cron("0 0 * * 7"), Cron { text: "0 0 * * 7".into(), ..cron("0 0 * * 0") });
}

#[test]
fn both_day_fields_restricted_match_either() {
    let first_or_monday = cron("0 0 1 * 1");
    assert_eq!(first_or_monday.next_after(FRIDAY), Some(FRIDAY + 3 * DAY));
    let first = cron("0 0 1 * *");
    assert_eq!(first.next_after(FRIDAY), Some(FRIDAY + 4 * DAY));
}

#[test]
fn a_leap_day_is_found_and_an_impossible_day_is_none() {
    let leap = cron("0 0 29 2 *").next_after(FRIDAY).unwrap();
    let date = crate::Date::from_unix_days(i64::try_from(leap / DAY).unwrap()).unwrap();
    assert_eq!(date.to_string(), "2028-02-29");
    assert_eq!(cron("0 0 30 2 *").next_after(FRIDAY), None);
}

#[test]
fn fires_at_is_the_minute_test_the_scheduler_uses() {
    let nine = cron("0 9 * * *");
    assert!(nine.fires_at(FRIDAY + 9 * HOUR));
    assert!(nine.fires_at(FRIDAY + 9 * HOUR + 59_999));
    assert!(!nine.fires_at(FRIDAY + 9 * HOUR + MINUTE));
}

#[test]
fn a_bad_cron_says_which_field_is_wrong() {
    let cases = [
        ("* * * *", "has 4 fields; a cron has 5"),
        ("60 * * * *", "`60` in the minute field is outside 0-59"),
        ("* 24 * * *", "`24` in the hour field is outside 0-23"),
        ("* * 0 * *", "`0` in the day of the month field is outside 1-31"),
        ("* * * 13 *", "`13` in the month field is outside 1-12"),
        ("* * * * 8", "`8` in the day of the week field is outside 0-7"),
        ("*/0 * * * *", "a step of 0 in the minute field"),
        ("MON * * * *", "`MON` in the minute field is not a number"),
        ("5-1 * * * *", "`5-1` in the minute field runs backwards"),
        ("@daily", "has 1 fields"),
    ];
    for (text, reason) in cases {
        let error = Cron::parse(text).unwrap_err();
        assert!(error.starts_with(reason), "{text}: {error}");
    }
}

#[test]
fn a_cron_is_written_back_as_text() {
    let cron = cron("0   9 * *   1-5");
    assert_eq!(cron.text(), "0 9 * * 1-5");
    assert_eq!(serde_json::to_value(&cron).unwrap(), "0 9 * * 1-5");
    let back: Cron = serde_json::from_value(serde_json::json!("0 9 * * 1-5")).unwrap();
    assert_eq!(back, cron);
    assert!(serde_json::from_value::<Cron>(serde_json::json!("nope")).is_err());
}

#[test]
fn a_stepped_day_field_restricts_the_day() {
    let every_other_day = cron("0 0 */2 * *");
    assert!(every_other_day.fires_at(FRIDAY + 2 * DAY), "the 29th is odd, so it fires");
    assert!(!every_other_day.fires_at(FRIDAY + 3 * DAY), "the 30th is even, so it does not");
    let every_other_weekday = cron("0 0 * * */2");
    assert!(!every_other_weekday.fires_at(FRIDAY), "Friday is day 5");
    assert!(every_other_weekday.fires_at(FRIDAY + 2 * DAY), "Sunday is day 0");
}

#[test]
fn a_stepped_day_of_the_month_and_a_weekday_match_either() {
    let stepped_or_monday = cron("0 0 */3 * 1");
    assert!(stepped_or_monday.fires_at(FRIDAY + 3 * DAY), "Monday the 30th fires by the weekday");
    assert!(stepped_or_monday.fires_at(FRIDAY + DAY), "Saturday the 28th matches */3 (1, 4, ... 28)");
    assert!(stepped_or_monday.fires_at(FRIDAY + 4 * DAY), "Tuesday 1 December matches */3");
    assert!(!stepped_or_monday.fires_at(FRIDAY + 2 * DAY), "Sunday the 29th matches neither");
    assert!(!stepped_or_monday.fires_at(FRIDAY + 5 * DAY), "Wednesday 2 December matches neither");
}

#[test]
fn a_start_with_a_step_of_one_runs_to_the_top_of_the_field() {
    let from_five = cron("0 5/1 * * *");
    assert!(from_five.fires_at(FRIDAY + 23 * HOUR));
    assert!(from_five.fires_at(FRIDAY + 5 * HOUR));
    assert!(!from_five.fires_at(FRIDAY + 4 * HOUR));
    assert_eq!(cron("0 5/1 * * *"), Cron { text: "0 5/1 * * *".into(), ..cron("0 5-23 * * *") });
}
