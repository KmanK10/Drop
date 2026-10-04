import Foundation

public enum DropFormat {
    public static func bytes(_ bytes: UInt64) -> String {
        if bytes < 1024 { return "\(bytes) B" }
        if bytes < 1024 * 1024 {
            let value = Double(bytes) / 1024
            return value < 10 ? String(format: "%.1f KB", value) : "\(UInt64(value.rounded())) KB"
        }
        if bytes < 1024 * 1024 * 1024 {
            let value = Double(bytes) / (1024 * 1024)
            return value < 10 ? String(format: "%.1f MB", value) : "\(UInt64(value.rounded())) MB"
        }
        let value = Double(bytes) / (1024 * 1024 * 1024)
        return value < 10 ? String(format: "%.1f GB", value) : "\(UInt64(value.rounded())) GB"
    }

    public static func retention(_ ttlMs: UInt64) -> String {
        let days = max(ttlMs / (24 * 60 * 60 * 1000), 1)
        return days == 1 ? "1 day" : "\(days) days"
    }

    /// Same words and clock as the desktop `format_when`: milliseconds since
    /// the Unix epoch, then just now, Ns ago, Nm ago, Nh ago, Nd ago, or a UTC date.
    public static func when(_ timestampMs: Int64, nowMs: Int64) -> String {
        let seconds = Int64((Double(nowMs - timestampMs) / 1000.0).rounded())
        if seconds < 15 {
            return "just now"
        }
        if seconds < 60 {
            return "\(seconds)s ago"
        }
        let minutes = Int64((Double(seconds) / 60.0).rounded())
        if minutes < 60 {
            return "\(minutes)m ago"
        }
        let hours = Int64((Double(minutes) / 60.0).rounded())
        if hours < 24 {
            return "\(hours)h ago"
        }
        let days = Int64((Double(hours) / 24.0).rounded())
        if days < 7 {
            return "\(days)d ago"
        }
        let daysSince = divEuclid(timestampMs, 86_400_000)
        let z = daysSince + 719_468
        let era = (z >= 0 ? z : z - 146_096) / 146_097
        let doe = UInt64(z - era * 146_097)
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365
        let y = Int64(yoe) + era * 400
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100)
        let mp = (5 * doy + 2) / 153
        let day = doy - (153 * mp + 2) / 5 + 1
        let monthNumber = Int64(mp) + (mp < 10 ? 3 : -9)
        let year = monthNumber <= 2 ? y + 1 : y
        let months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"]
        let monthIndex = Int(monthNumber) - 1
        let month = months.indices.contains(monthIndex) ? months[monthIndex] : "?"
        if year == civilYear(nowMs) {
            return "\(month) \(day)"
        }
        return "\(month) \(day), \(year)"
    }

    /// Days until `createdAt + ttlMs`. Nil at 4 days or more, or when there is no lifetime.
    /// This is the real time left, not the age from `when`.
    public static func daysLeft(createdAt: Int64, ttlMs: UInt64, nowMs: Int64) -> String? {
        guard ttlMs > 0, let ttl = Int64(exactly: ttlMs) else { return nil }
        let (expires, overflow) = createdAt.addingReportingOverflow(ttl)
        guard !overflow else { return nil }
        let (remaining, remainingOverflow) = expires.subtractingReportingOverflow(nowMs)
        guard !remainingOverflow else { return nil }
        let day: Int64 = 86_400_000
        if remaining >= 4 * day { return nil }
        if remaining <= 0 { return "0 days left" }
        let days = remaining / day
        if days <= 0 { return "Less than a day" }
        if days == 1 { return "1 day left" }
        return "\(days) days left"
    }

    public static func nowMs() -> Int64 {
        Int64(Date().timeIntervalSince1970 * 1000.0)
    }

    private static func civilYear(_ timestampMs: Int64) -> Int64 {
        let daysSince = divEuclid(timestampMs, 86_400_000)
        let z = daysSince + 719_468
        let era = (z >= 0 ? z : z - 146_096) / 146_097
        let doe = UInt64(z - era * 146_097)
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365
        let y = Int64(yoe) + era * 400
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100)
        let mp = (5 * doy + 2) / 153
        let monthNumber = Int64(mp) + (mp < 10 ? 3 : -9)
        return monthNumber <= 2 ? y + 1 : y
    }

    /// Floor division, matching Rust `div_euclid`.
    private static func divEuclid(_ value: Int64, _ divisor: Int64) -> Int64 {
        let quotient = value / divisor
        let remainder = value % divisor
        if remainder != 0 && (remainder < 0) != (divisor < 0) {
            return quotient - 1
        }
        return quotient
    }

    static func preview(_ text: String, limit: Int) -> String {
        let flat = text.split(whereSeparator: \.isWhitespace).joined(separator: " ")
        if flat.count <= limit { return flat }
        let end = flat.index(flat.startIndex, offsetBy: limit)
        return String(flat[..<end]) + "…"
    }
}
