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

/// The last component of a path, whichever separator it uses and with trailing ones ignored:
/// `D:\Dati\Foto\` -> `Foto`. A drive root (`E:\`) has no folder name, so it is returned as given.
pub fn folder_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['\\', '/']);
    match trimmed.rsplit(['\\', '/']).next() {
        Some(name) if !name.is_empty() && trimmed.len() != path.len().min(2) => name.to_string(),
        _ => path.to_string(),
    }
}

/// Names for a list row: the first two folder names, then `+N` for the rest.
pub fn folder_names(paths: &[String]) -> String {
    let mut names: Vec<String> = paths.iter().take(2).map(|p| folder_name(p)).collect();
    if paths.len() > 2 {
        names.push(format!("+{}", paths.len() - 2));
    }
    names.join(", ")
}

/// `2026-10-08 12:19 UTC` -> the local `08/10/2026 14:19`.
pub fn when_text(at: chrono::DateTime<chrono::Utc>) -> String {
    at.with_timezone(&chrono::Local)
        .format("%d/%m/%Y %H:%M")
        .to_string()
}

/// `08/10 14:19`, the short form for a list row where space is tight.
pub fn when_short(at: chrono::DateTime<chrono::Utc>) -> String {
    at.with_timezone(&chrono::Local)
        .format("%d/%m %H:%M")
        .to_string()
}

/// Average MB/s over a whole session (all its reports); `0` when the time is unknown.
pub fn throughput_mbps(bytes: u64, seconds: f64) -> f64 {
    if seconds > 0.0 {
        bytes as f64 / (1024.0 * 1024.0) / seconds
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names_ignore_separators_and_trailing_slashes() {
        assert_eq!(folder_name(r"D:\Dati\Foto"), "Foto");
        assert_eq!(folder_name(r"D:\Dati\Foto\"), "Foto");
        assert_eq!(folder_name("D:/Dati/Video/"), "Video");
        assert_eq!(folder_name(r"\\nas01\backup"), "backup");
    }

    #[test]
    fn a_drive_root_keeps_its_own_spelling() {
        assert_eq!(folder_name(r"E:\"), r"E:\");
    }

    #[test]
    fn a_long_list_of_folders_is_shortened() {
        let paths: Vec<String> = ["A", "B", "C", "D"]
            .iter()
            .map(|n| format!(r"C:\{n}"))
            .collect();
        assert_eq!(folder_names(&paths[..1]), "A");
        assert_eq!(folder_names(&paths[..2]), "A, B");
        assert_eq!(folder_names(&paths), "A, B, +2");
    }

    #[test]
    fn throughput_is_zero_when_the_time_is_unknown() {
        assert_eq!(throughput_mbps(1024 * 1024 * 100, 0.0), 0.0);
        assert_eq!(throughput_mbps(1024 * 1024 * 100, 2.0), 50.0);
    }

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
