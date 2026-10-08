//! Number formatting for the interface (Italian decimal comma). Pure functions, unit-tested.

/// `1536` -> `"1,5 KB"`; whole bytes stay whole (`"512 B"`).
pub fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit]).replace('.', ",")
    }
}

/// `75.0` -> `"1 min 15 s"`; under a minute `"12 s"`.
pub fn human_duration(seconds: f64) -> String {
    let total = seconds.max(0.0).round() as u64;
    if total < 60 {
        format!("{total} s")
    } else {
        format!("{} min {:02} s", total / 60, total % 60)
    }
}

/// Throughput as the report shows it: whole MB/s, `"0 MB/s"` when unknown.
pub fn human_speed(mbps: f64) -> String {
    format!(
        "{:.0} MB/s",
        if mbps.is_finite() { mbps.max(0.0) } else { 0.0 }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_use_a_decimal_comma_and_stay_whole_below_one_kb() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1,5 KB");
        assert_eq!(human_bytes(126_000_000), "120,2 MB");
        assert!(human_bytes(u64::MAX).ends_with("TB"));
    }

    #[test]
    fn durations_switch_to_minutes_at_sixty_seconds() {
        assert_eq!(human_duration(0.4), "0 s");
        assert_eq!(human_duration(59.4), "59 s");
        assert_eq!(human_duration(75.0), "1 min 15 s");
        assert_eq!(human_duration(-3.0), "0 s");
    }

    #[test]
    fn speed_never_prints_nan_or_infinity() {
        assert_eq!(human_speed(f64::NAN), "0 MB/s");
        assert_eq!(human_speed(f64::INFINITY), "0 MB/s");
        assert_eq!(human_speed(1027.4), "1027 MB/s");
    }
}
