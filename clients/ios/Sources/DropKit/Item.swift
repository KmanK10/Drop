import Foundation

enum ItemKind: String {
    case text
    case file
}

struct ItemPlain {
    var kind: ItemKind
    var name: String
    var mime: String
    var body: [UInt8]
}

enum ItemCodec {
    static func encode(_ item: ItemPlain) throws -> [UInt8] {
        let name = Array(item.name.utf8)
        let mime = Array(item.mime.utf8)
        if name.count > 512 { throw DropError.nameTooLong }
        if mime.count > 200 { throw DropError.mimeTooLong }
        var out: [UInt8] = [0x44, 0x52, 0x50, 0x31]
        out.append(item.kind == .text ? 1 : 2)
        out.append(UInt8((name.count >> 8) & 0xFF))
        out.append(UInt8(name.count & 0xFF))
        out.append(contentsOf: name)
        out.append(UInt8((mime.count >> 8) & 0xFF))
        out.append(UInt8(mime.count & 0xFF))
        out.append(contentsOf: mime)
        out.append(contentsOf: item.body)
        return out
    }

    static func decode(_ bytes: [UInt8]) throws -> ItemPlain {
        guard bytes.count >= 4 + 1 + 2 + 2 else { throw DropError.truncatedItem }
        guard bytes[0] == 0x44, bytes[1] == 0x52, bytes[2] == 0x50, bytes[3] == 0x31 else {
            throw DropError.badItem
        }
        let kind: ItemKind
        switch bytes[4] {
        case 1: kind = .text
        case 2: kind = .file
        default: throw DropError.badItem
        }
        let nameLen = Int(bytes[5]) << 8 | Int(bytes[6])
        var offset = 7
        guard offset + nameLen + 2 <= bytes.count else { throw DropError.truncatedItem }
        let name = String(decoding: bytes[offset..<(offset + nameLen)], as: UTF8.self)
        offset += nameLen
        let mimeLen = Int(bytes[offset]) << 8 | Int(bytes[offset + 1])
        offset += 2
        guard offset + mimeLen <= bytes.count else { throw DropError.truncatedItem }
        let mime = String(decoding: bytes[offset..<(offset + mimeLen)], as: UTF8.self)
        offset += mimeLen
        return ItemPlain(kind: kind, name: name, mime: mime, body: Array(bytes[offset...]))
    }

    static func safeDownloadName(_ name: String) -> String {
        var base = name.replacingOccurrences(of: "/", with: "").replacingOccurrences(of: "\\", with: "")
        while base.hasPrefix(".") {
            base.removeFirst()
        }
        let trimmed = String(base.trimmingCharacters(in: .whitespacesAndNewlines).prefix(180))
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? "download" : trimmed
    }
}
