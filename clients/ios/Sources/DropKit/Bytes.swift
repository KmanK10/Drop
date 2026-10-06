import CryptoKit
import Foundation

enum Bytes {
    static func b64url(_ bytes: [UInt8]) -> String {
        let table = Array("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_".utf8)
        var out = ""
        var index = 0
        while index + 3 <= bytes.count {
            let n = (UInt32(bytes[index]) << 16) | (UInt32(bytes[index + 1]) << 8) | UInt32(bytes[index + 2])
            out.append(Character(UnicodeScalar(table[Int((n >> 18) & 63)])))
            out.append(Character(UnicodeScalar(table[Int((n >> 12) & 63)])))
            out.append(Character(UnicodeScalar(table[Int((n >> 6) & 63)])))
            out.append(Character(UnicodeScalar(table[Int(n & 63)])))
            index += 3
        }
        if index < bytes.count {
            var n = UInt32(bytes[index]) << 16
            out.append(Character(UnicodeScalar(table[Int((n >> 18) & 63)])))
            if index + 1 < bytes.count {
                n |= UInt32(bytes[index + 1]) << 8
                out.append(Character(UnicodeScalar(table[Int((n >> 12) & 63)])))
                out.append(Character(UnicodeScalar(table[Int((n >> 6) & 63)])))
            } else {
                out.append(Character(UnicodeScalar(table[Int((n >> 12) & 63)])))
            }
        }
        return out
    }

    static func fromB64url(_ value: String) -> [UInt8]? {
        let bytes = Array(value.utf8)
        if bytes.contains(where: { byte in
            !(byte == UInt8(ascii: "-") || byte == UInt8(ascii: "_")
                || (0x30...0x39).contains(byte) || (0x41...0x5A).contains(byte) || (0x61...0x7A).contains(byte))
        }) {
            return nil
        }
        func val(_ byte: UInt8) -> UInt8 {
            switch byte {
            case 0x41...0x5A: return byte - 0x41
            case 0x61...0x7A: return byte - 0x61 + 26
            case 0x30...0x39: return byte - 0x30 + 52
            case UInt8(ascii: "-"): return 62
            default: return 63
            }
        }
        var out: [UInt8] = []
        var index = 0
        while index + 4 <= bytes.count {
            let n = (UInt32(val(bytes[index])) << 18)
                | (UInt32(val(bytes[index + 1])) << 12)
                | (UInt32(val(bytes[index + 2])) << 6)
                | UInt32(val(bytes[index + 3]))
            out.append(UInt8((n >> 16) & 0xFF))
            out.append(UInt8((n >> 8) & 0xFF))
            out.append(UInt8(n & 0xFF))
            index += 4
        }
        let rest = bytes.count - index
        if rest == 1 { return nil }
        if rest >= 2 {
            var n = (UInt32(val(bytes[index])) << 18) | (UInt32(val(bytes[index + 1])) << 12)
            out.append(UInt8((n >> 16) & 0xFF))
            if rest == 3 {
                n |= UInt32(val(bytes[index + 2])) << 6
                out.append(UInt8((n >> 8) & 0xFF))
            }
        }
        return out
    }

    static func sha256(_ bytes: [UInt8]) -> [UInt8] {
        Array(SHA256.hash(data: Data(bytes)))
    }

    static func timingEqual(_ a: [UInt8], _ b: [UInt8]) -> Bool {
        guard a.count == b.count else { return false }
        var diff: UInt8 = 0
        for index in a.indices {
            diff |= a[index] ^ b[index]
        }
        return diff == 0
    }

    static func wipe(_ bytes: inout [UInt8]) {
        for index in bytes.indices {
            bytes[index] = 0
        }
        bytes.removeAll(keepingCapacity: false)
    }
}
