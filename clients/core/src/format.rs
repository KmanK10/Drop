pub fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    if bytes < 1024 * 1024 {
        let value = bytes as f64 / 1024.0;
        if value < 10.0 {
            return format!("{value:.1} KB");
        }
        return format!("{} KB", value.round() as u64);
    }
    if bytes < 1024 * 1024 * 1024 {
        let value = bytes as f64 / (1024.0 * 1024.0);
        if value < 10.0 {
            return format!("{value:.1} MB");
        }
        return format!("{} MB", value.round() as u64);
    }
    let value = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    if value < 10.0 {
        format!("{value:.1} GB")
    } else {
        format!("{} GB", value.round() as u64)
    }
}

pub fn retention_label(ttl_ms: u64) -> String {
    let days = (ttl_ms / (24 * 60 * 60 * 1000)).max(1);
    if days == 1 {
        "1 day".into()
    } else {
        format!("{days} days")
    }
}

/// Days until `created_at_ms + ttl_ms`, using the real remaining time.
/// `None` at 4 days or more, or when there is no lifetime. The age words
/// from `format_when` are a separate clock and are not used here.
pub fn days_left(created_at_ms: i64, ttl_ms: u64, now_ms: i64) -> Option<String> {
    if ttl_ms == 0 {
        return None;
    }
    let Ok(ttl) = i64::try_from(ttl_ms) else {
        return None;
    };
    let Some(expires) = created_at_ms.checked_add(ttl) else {
        return None;
    };
    let remaining = expires as i128 - now_ms as i128;
    let day: i128 = 86_400_000;
    if remaining >= 4 * day {
        return None;
    }
    if remaining <= 0 {
        return Some("0 days left".into());
    }
    let days = remaining / day;
    if days <= 0 {
        return Some("Less than a day".into());
    }
    if days == 1 {
        return Some("1 day left".into());
    }
    Some(format!("{days} days left"))
}

pub fn format_when(timestamp_ms: i64, now_ms: i64) -> String {
    let seconds = ((now_ms - timestamp_ms) as f64 / 1000.0).round() as i64;
    if seconds < 15 {
        return "just now".into();
    }
    if seconds < 60 {
        return format!("{seconds}s ago");
    }
    let minutes = (seconds as f64 / 60.0).round() as i64;
    if minutes < 60 {
        return format!("{minutes}m ago");
    }
    let hours = (minutes as f64 / 60.0).round() as i64;
    if hours < 24 {
        return format!("{hours}h ago");
    }
    let days = (hours as f64 / 24.0).round() as i64;
    if days < 7 {
        return format!("{days}d ago");
    }
    let days_since = timestamp_ms.div_euclid(86_400_000);
    let z = days_since + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp as i64 + if mp < 10 { 3 } else { -9 };
    let year = if m <= 2 { y + 1 } else { y };
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month = MONTHS.get((m as usize).wrapping_sub(1)).copied().unwrap_or("?");
    if year == civil_year(now_ms) {
        format!("{month} {d}")
    } else {
        format!("{month} {d}, {year}")
    }
}

fn civil_year(timestamp_ms: i64) -> i64 {
    let days_since = timestamp_ms.div_euclid(86_400_000);
    let z = days_since + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = mp as i64 + if mp < 10 { 3 } else { -9 };
    if m <= 2 {
        y + 1
    } else {
        y
    }
}

pub fn text_preview(text: &str, limit: usize) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let count = flat.chars().count();
    if count <= limit {
        return flat;
    }
    let mut out: String = flat.chars().take(limit.saturating_sub(1)).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::{days_left, format_when};

    #[test]
    fn days_left_warns_only_under_four_days() {
        let day = 86_400_000_i64;
        let ttl = (30 * day) as u64;
        let created = 1_000_000_000_000_i64;
        let expires = created + 30 * day;
        assert_eq!(days_left(created, ttl, expires - 4 * day), None);
        assert_eq!(days_left(created, ttl, expires - 4 * day + 1), Some("3 days left".into()));
        assert_eq!(days_left(created, ttl, expires - 3 * day), Some("3 days left".into()));
        assert_eq!(days_left(created, ttl, expires - 2 * day), Some("2 days left".into()));
        assert_eq!(days_left(created, ttl, expires - day), Some("1 day left".into()));
        assert_eq!(days_left(created, ttl, expires - day + 1), Some("Less than a day".into()));
        assert_eq!(days_left(created, ttl, expires - 12 * 60 * 60 * 1000), Some("Less than a day".into()));
        assert_eq!(days_left(created, ttl, expires), Some("0 days left".into()));
        assert_eq!(days_left(created, ttl, expires + 1), Some("0 days left".into()));
        assert_eq!(days_left(created, 0, expires - day), None);
    }

    #[test]
    fn age_uses_the_desktop_words() {
        let now = 1_700_000_000_000_i64;
        assert_eq!(format_when(now, now), "just now");
        assert_eq!(format_when(now - 14_000, now), "just now");
        assert_eq!(format_when(now - 14_499, now), "just now");
        assert_eq!(format_when(now - 14_500, now), "15s ago");
        assert_eq!(format_when(now - 15_000, now), "15s ago");
        assert_eq!(format_when(now - 59_000, now), "59s ago");
        assert_eq!(format_when(now - 60_000, now), "1m ago");
        assert_eq!(format_when(now - 90_000, now), "2m ago");
        assert_eq!(format_when(now - 59 * 60_000, now), "59m ago");
        assert_eq!(format_when(now - 60 * 60_000, now), "1h ago");
        assert_eq!(format_when(now - 23 * 60 * 60_000, now), "23h ago");
        assert_eq!(format_when(now - 24 * 60 * 60_000, now), "1d ago");
        assert_eq!(format_when(now - 6 * 24 * 60 * 60_000, now), "6d ago");
        assert_eq!(format_when(now - 7 * 24 * 60 * 60_000, now), "Nov 7");
        assert_eq!(format_when(now - 8 * 24 * 60 * 60_000, now), "Nov 6");
        assert_eq!(format_when(now - 40 * 24 * 60 * 60_000, now), "Oct 5");
        assert_eq!(format_when(1_669_852_800_000, now), "Dec 1, 2022");
    }
}

pub fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
