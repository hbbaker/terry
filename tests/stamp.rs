use terry::stamp::stamp_of;

#[test]
fn should_stamp_the_epoch_when_seconds_are_zero() {
    assert_eq!(stamp_of(0), "19700101-000000");
}

#[test]
fn should_stamp_the_utc_date_and_time_when_seconds_are_given() {
    // `date -u -d @1792778400 +%Y%m%d-%H%M%S`
    assert_eq!(stamp_of(1_792_778_400), "20261023-180000");
}

#[test]
fn should_stamp_february_29_when_the_year_is_a_leap_year() {
    // `date -u -d @1709208000 +%Y%m%d-%H%M%S`
    assert_eq!(stamp_of(1_709_208_000), "20240229-120000");
}
