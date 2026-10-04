import Foundation

enum ClipboardKind: Equatable {
    case text
    case html
    case image
}

enum ClipboardKinds {
    private static let textTypes: Set<String> = [
        "application/json",
        "application/xml",
        "application/javascript",
        "application/x-javascript",
        "application/ecmascript",
        "application/sql",
        "application/graphql",
        "application/yaml",
        "application/x-yaml",
        "application/toml",
        "application/x-toml",
        "application/x-sh",
        "application/csv",
        "image/svg+xml",
    ]

    static func baseMime(_ mime: String) -> String {
        mime.split(separator: ";", maxSplits: 1).first.map { $0.trimmingCharacters(in: .whitespaces).lowercased() } ?? ""
    }

    static func fileKind(_ mime: String) -> ClipboardKind? {
        let base = baseMime(mime)
        if base.isEmpty || base == "application/octet-stream" { return nil }
        if base == "text/html" || base == "application/xhtml+xml" { return .html }
        if base.hasPrefix("text/") || textTypes.contains(base) || (base.hasPrefix("application/") && (base.hasSuffix("+json") || base.hasSuffix("+xml"))) {
            return .text
        }
        if base.hasPrefix("image/") { return .image }
        return nil
    }
}
