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

/// Average MB/s over a whole session (all its reports); `0` when the time is unknown. MB is 10^6
/// bytes, the same unit the core uses everywhere (`progress::throughput_mbps`), so the figure here
/// and the one in a report cannot disagree.
pub fn throughput_mbps(bytes: u64, seconds: f64) -> f64 {
    if seconds > 0.0 {
        bytes as f64 / 1_000_000.0 / seconds
    } else {
        0.0
    }
}

/// How many samples the speed chart holds: 120 ticks of 250 ms, the last 30 seconds.
pub const CHART_SAMPLES: usize = 120;

/// The SVG path of a speed chart in a `width` x `height` box: the newest sample at the right edge,
/// scaled to the fastest sample seen (never to zero, so a flat line stays flat). Empty until there
/// are two samples to join.
pub fn chart_path(samples: &[f64], width: f64, height: f64) -> String {
    // Nothing to draw until something has actually moved: a flat line at the bottom would read as
    // "stuck at zero", which is a claim, not an absence of data.
    if samples.len() < 2 || !samples.iter().any(|v| v.is_finite() && *v > 0.0) {
        return String::new();
    }
    let step = width / (CHART_SAMPLES as f64 - 1.0);
    let first_x = width - step * (samples.len() as f64 - 1.0);
    let peak = samples
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(1.0_f64, f64::max);
    let mut path = String::new();
    for (index, value) in samples.iter().enumerate() {
        let value = if value.is_finite() {
            value.max(0.0)
        } else {
            0.0
        };
        let x = first_x + step * index as f64;
        let y = height - (value / peak) * (height - 2.0) - 1.0;
        path.push_str(if index == 0 { "M " } else { " L " });
        path.push_str(&format!("{x:.1} {y:.1}"));
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chart_needs_two_samples_and_grows_from_the_right() {
        assert_eq!(chart_path(&[], 240.0, 56.0), "");
        assert_eq!(chart_path(&[5.0], 240.0, 56.0), "");
        let two = chart_path(&[0.0, 10.0], 240.0, 56.0);
        assert!(two.starts_with("M "), "{two}");
        assert!(
            two.contains(" L 240.0 "),
            "the newest sample is at the right edge: {two}"
        );
    }

    #[test]
    fn a_chart_scales_to_the_fastest_sample_and_survives_bad_numbers() {
        let path = chart_path(&[0.0, 50.0, f64::NAN, -3.0, 100.0], 240.0, 56.0);
        assert!(!path.contains("NaN") && !path.contains("inf"), "{path}");
        // The fastest sample is drawn at the top, the slowest at the bottom, inside the box.
        assert!(
            path.contains(" 1.0"),
            "the peak touches the top margin: {path}"
        );
        assert!(
            path.contains(" 55.0"),
            "zero sits on the bottom margin: {path}"
        );
    }

    #[test]
    fn a_chart_of_nothing_is_not_drawn() {
        assert_eq!(chart_path(&[0.0, 0.0, 0.0], 240.0, 56.0), "");
        assert_eq!(chart_path(&[f64::NAN, 0.0], 240.0, 56.0), "");
    }

    #[test]
    fn a_chart_that_has_moved_and_then_idled_keeps_its_shape() {
        let path = chart_path(&[0.0, 40.0, 0.0], 240.0, 56.0);
        assert_eq!(path.matches(" 55.0").count(), 2, "{path}");
    }

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
        assert_eq!(throughput_mbps(100_000_000, 0.0), 0.0);
        assert_eq!(throughput_mbps(100_000_000, 2.0), 50.0);
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
