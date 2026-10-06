import CoreGraphics
import Foundation
import ImageIO

/// In-memory previews. The decrypted item is not written to a file or the app-group inbox.
enum ItemPreview {
    /// A short plain-text snippet. Only the start of a large body is read.
    static func textSnippet(_ bytes: [UInt8], limit: Int = 160) -> String {
        let end = min(bytes.count, 8_192)
        let sample = String(decoding: bytes.prefix(end), as: UTF8.self)
        let withoutTags = sample.replacingOccurrences(
            of: "<[^>]{0,300}>",
            with: " ",
            options: .regularExpression
        )
        var snippet = DropFormat.preview(withoutTags, limit: limit)
        if bytes.count > end, !snippet.isEmpty, !snippet.hasSuffix("…") {
            snippet += "…"
        }
        return snippet
    }

    /// A small JPEG for a photo. Returns nil when the bytes are not a readable image.
    static func thumbnailJPEG(_ bytes: [UInt8], maxPixel: Int = 320) -> Data? {
        guard !bytes.isEmpty, bytes.count <= 40 * 1024 * 1024 else { return nil }
        let data = Data(bytes)
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: maxPixel,
            kCGImageSourceShouldCacheImmediately: true,
        ]
        guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary) else { return nil }
        let out = NSMutableData()
        guard let dest = CGImageDestinationCreateWithData(out, "public.jpeg" as CFString, 1, nil) else { return nil }
        let quality = [kCGImageDestinationLossyCompressionQuality: 0.72] as CFDictionary
        CGImageDestinationAddImage(dest, image, quality)
        guard CGImageDestinationFinalize(dest) else { return nil }
        return out as Data
    }
}
