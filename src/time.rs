pub fn parse_utc_millis(timestamp: &str) -> Option<u64> {
    let field = |range: std::ops::Range<usize>| timestamp.get(range)?.parse::<i64>().ok();
    let (year, month, day) = (field(0..4)?, field(5..7)?, field(8..10)?);
    let (hour, minute, second) = (field(11..13)?, field(14..16)?, field(17..19)?);
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    let seconds = days * 86_400 + hour * 3_600 + minute * 60 + second;
    u64::try_from(seconds).ok().map(|s| s * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rfc3339_utc_timestamps() {
        assert_eq!(parse_utc_millis("1970-01-02T00:00:00Z"), Some(86_400_000));
        assert_eq!(parse_utc_millis("2000-03-01T00:00:00.123Z"), Some(951_868_800_000));
        assert_eq!(parse_utc_millis("2026-10-08T05:52:12Z"), Some(1_791_438_732_000));
        assert_eq!(parse_utc_millis("garbage"), None);
    }
}
