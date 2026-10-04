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

    static func preview(_ text: String, limit: Int) -> String {
        let flat = text.split(whereSeparator: \.isWhitespace).joined(separator: " ")
        if flat.count <= limit { return flat }
        let end = flat.index(flat.startIndex, offsetBy: limit)
        return String(flat[..<end]) + "…"
    }
}
