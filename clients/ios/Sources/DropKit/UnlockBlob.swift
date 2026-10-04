import Foundation

/// In-memory blob for a biometric keychain. The password is not in it.
/// Layout: `DRPK`, version 1, 32-byte content key, then server, username, and
/// session cookie as little-endian length-prefixed UTF-8.
public struct UnlockBlob: Equatable {
    public var contentKey: [UInt8]
    public var server: String
    public var username: String
    public var cookie: String

    public init(contentKey: [UInt8], server: String, username: String, cookie: String) {
        self.contentKey = contentKey
        self.server = server
        self.username = username
        self.cookie = cookie
    }

    public func encode() throws -> Data {
        guard contentKey.count == 32, !server.isEmpty, server.utf8.count <= 500,
              !username.isEmpty, username.utf8.count <= 64,
              !cookie.isEmpty, cookie.utf8.count <= 4096
        else { throw DropError.message("Couldn't store biometric unlock.") }
        var out = Data("DRPK".utf8)
        out.append(1)
        out.append(contentsOf: contentKey)
        try append(&out, server)
        try append(&out, username)
        try append(&out, cookie)
        return out
    }

    public static func decode(_ data: Data) throws -> UnlockBlob {
        let bytes = [UInt8](data)
        guard bytes.count >= 4 + 1 + 32 + 6, bytes.starts(with: [UInt8]("DRPK".utf8)), bytes[4] == 1 else {
            throw DropError.message("Couldn't unlock with biometrics.")
        }
        let key = Array(bytes[5..<37])
        var index = 37
        let server = try take(bytes, &index)
        let username = try take(bytes, &index)
        let cookie = try take(bytes, &index)
        guard index == bytes.count, !server.isEmpty, !username.isEmpty, !cookie.isEmpty else {
            throw DropError.message("Couldn't unlock with biometrics.")
        }
        return UnlockBlob(contentKey: key, server: server, username: username, cookie: cookie)
    }

    private static func take(_ bytes: [UInt8], _ index: inout Int) throws -> String {
        guard index + 2 <= bytes.count else { throw DropError.message("Couldn't unlock with biometrics.") }
        let len = Int(bytes[index]) | (Int(bytes[index + 1]) << 8)
        index += 2
        guard len <= 4096, index + len <= bytes.count else {
            throw DropError.message("Couldn't unlock with biometrics.")
        }
        let text = String(decoding: bytes[index..<(index + len)], as: UTF8.self)
        index += len
        guard text.utf8.count == len else { throw DropError.message("Couldn't unlock with biometrics.") }
        return text
    }
}

private func append(_ out: inout Data, _ text: String) throws {
    let utf8 = Array(text.utf8)
    guard utf8.count <= Int(UInt16.max) else { throw DropError.message("Couldn't store biometric unlock.") }
    out.append(UInt8(utf8.count & 0xff))
    out.append(UInt8((utf8.count >> 8) & 0xff))
    out.append(contentsOf: utf8)
}
