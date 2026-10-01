pub fn format_duration(duration_ms: Option<i64>) -> String {
    let Some(ms) = duration_ms.filter(|ms| *ms > 0) else {
        return "00:00".to_owned();
    };
    let total = ms / 1000;
    let (hours, minutes, seconds) = (total / 3600, (total % 3600) / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes:02}:{seconds:02}")
    }
}

pub fn format_bytes(size_bytes: u64) -> String {
    if size_bytes == 0 {
        return "0 B".to_owned();
    }
    let mut size = size_bytes as f64;
    for unit in ["B", "KB", "MB"] {
        if size < 1024.0 {
            return format!("{size:.2} {unit}");
        }
        size /= 1024.0;
    }
    format!("{size:.2} GB")
}

pub fn mask_secret(value: &str) -> String {
    const VISIBLE: usize = 4;
    let chars: Vec<char> = value.chars().collect();
    if chars.len() <= VISIBLE * 2 {
        return "*".repeat(chars.len());
    }
    let head: String = chars[..VISIBLE].iter().collect();
    let tail: String = chars[chars.len() - VISIBLE..].iter().collect();
    format!("{head}...{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(format_duration(None), "00:00");
        assert_eq!(format_duration(Some(-5)), "00:00");
        assert_eq!(format_duration(Some(61_000)), "01:01");
        assert_eq!(format_duration(Some(3_723_000)), "1:02:03");
    }

    #[test]
    fn bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512.00 B");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024 * 1024 * 1024), "5120.00 GB");
    }

    #[test]
    fn masks() {
        assert_eq!(mask_secret("abcdefgh"), "********");
        assert_eq!(mask_secret("abcdefghijkl"), "abcd...ijkl");
    }

    #[test]
    fn bytes_switch_unit_exactly_at_1024() {
        assert_eq!(format_bytes(1023), "1023.00 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
    }
}
