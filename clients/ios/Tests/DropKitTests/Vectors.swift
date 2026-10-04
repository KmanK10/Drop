import CryptoKit
import Foundation
import XCTest
@testable import DropKit

final class Vectors: XCTestCase {
    private let argonVector = "4bd8ebfb202d2c08d9f467099ed686f4d05dc192458b76b0b4d925526abbed76607e940d53e0478f9bbffe3314fb22164beedacb4171bd29807fbbc54a760323"

    func testArgon2idSplitMatchesTheBrowser() throws {
        let salt = [UInt8](repeating: 0x11, count: 16)
        let derived = try DropCrypto.deriveKeys(password: "drop-test-password", salt: salt, params: currentParams())
        let raw = hexDecode(argonVector)
        XCTAssertEqual(derived.contentKey, Array(raw[32...]))
        XCTAssertEqual(derived.authVerifier, Bytes.sha256(Array(raw[..<32])))
        XCTAssertNotEqual(derived.authVerifier, derived.contentKey)
    }

    func testPasswordsAreNormalized() throws {
        let salt = [UInt8](repeating: 0x22, count: 16)
        let latin = try DropCrypto.deriveKeys(password: "password1", salt: salt, params: currentParams())
        let fullwidth = try DropCrypto.deriveKeys(password: "password\u{FF11}", salt: salt, params: currentParams())
        let other = try DropCrypto.deriveKeys(password: "password2", salt: salt, params: currentParams())
        XCTAssertEqual(latin.contentKey, fullwidth.contentKey)
        XCTAssertEqual(latin.authVerifier, fullwidth.authVerifier)
        XCTAssertNotEqual(latin.contentKey, other.contentKey)
    }

    func testWeakKdfIsRefused() {
        var weak = currentParams()
        weak.memory = 1024
        XCTAssertThrowsError(try DropCrypto.deriveKeys(password: "drop-test-password", salt: [UInt8](repeating: 0x11, count: 16), params: weak)) { error in
            XCTAssertEqual(error as? DropError, .weakKdf)
        }
    }

    func testAesGcmLayoutMatchesTheKnownVector() throws {
        let key = hexDecode("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f")
        let blob = hexDecode("000102030405060708090a0b2370b96be88ea762a022ffeed282551bb2c5e283ae92bb8b612327f9d0e65adde0")
        let plain = try DropCrypto.decrypt(key: key, blob: blob)
        XCTAssertEqual(plain, Array("drop-key-check-v1".utf8))
        let round = try DropCrypto.encrypt(key: key, plaintext: Array("meet at the north door".utf8))
        XCTAssertEqual(try DropCrypto.decrypt(key: key, blob: round), Array("meet at the north door".utf8))
        var tampered = round
        tampered[tampered.count - 1] ^= 0xFF
        XCTAssertThrowsError(try DropCrypto.decrypt(key: key, blob: tampered))
    }

    func testItemCodecMatchesTheBrowser() throws {
        let encoded = try ItemCodec.encode(ItemPlain(kind: .text, name: "", mime: "text/plain", body: Array("hi".utf8)))
        XCTAssertEqual(hexEncode(encoded), "44525031010000000a746578742f706c61696e6869")
        let decoded = try ItemCodec.decode(encoded)
        XCTAssertEqual(decoded.kind, .text)
        XCTAssertEqual(decoded.body, Array("hi".utf8))
    }

    func testClipboardKindsMatchTheBrowser() {
        XCTAssertEqual(ClipboardKinds.fileKind("image/png"), .image)
        XCTAssertEqual(ClipboardKinds.fileKind("IMAGE/JPEG"), .image)
        XCTAssertEqual(ClipboardKinds.fileKind("text/plain; charset=utf-8"), .text)
        XCTAssertEqual(ClipboardKinds.fileKind("application/ld+json"), .text)
        XCTAssertEqual(ClipboardKinds.fileKind("image/svg+xml"), .text)
        XCTAssertEqual(ClipboardKinds.fileKind(" Text/HTML ; charset=UTF-8"), .html)
        for mime in ["application/pdf", "application/zip", "application/octet-stream", "video/mp4", ""] {
            XCTAssertNil(ClipboardKinds.fileKind(mime), mime)
        }
    }

    func testSettingsFileDropsSecrets() throws {
        let dir = URL(fileURLWithPath: NSTemporaryDirectory()).appendingPathComponent("drop-settings-\(ProcessInfo.processInfo.processIdentifier)")
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let path = dir.appendingPathComponent("config.json")
        let planted = #"{"serverUrl":"https://drop.example","username":"ada","password":"super-secret-password","contentKey":"not-a-real-key"}"#
        try planted.data(using: .utf8)?.write(to: path)
        let settings = DropSettings.load(from: path)
        XCTAssertEqual(settings.username, "ada")
        XCTAssertEqual(settings.serverURL, "https://drop.example")
        XCTAssertEqual(DropSettings.empty.serverURL, "")
        XCTAssertFalse(DropSettings.defaultServer.contains("kiefermenard"))
        let text = try String(contentsOf: path, encoding: .utf8)
        XCTAssertFalse(text.contains("super-secret-password"))
        XCTAssertFalse(text.contains("contentKey"))
        XCTAssertFalse(text.contains("not-a-real-key"))
        try FileManager.default.removeItem(at: dir)
    }

    func testCryptoKitCombinedLayoutIsNonceCiphertextTag() throws {
        let key = SymmetricKey(data: Data(hexDecode("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f")))
        let blob = Data(hexDecode("000102030405060708090a0b2370b96be88ea762a022ffeed282551bb2c5e283ae92bb8b612327f9d0e65adde0"))
        let box = try AES.GCM.SealedBox(combined: blob)
        let plain = try AES.GCM.open(box, using: key)
        XCTAssertEqual(String(decoding: plain, as: UTF8.self), "drop-key-check-v1")
    }

    private func currentParams() -> DropCrypto.Params {
        DropCrypto.Params(algo: "argon2id", memory: DropCrypto.memory, time: DropCrypto.time, parallelism: DropCrypto.parallelism)
    }

    private func hexDecode(_ value: String) -> [UInt8] {
        stride(from: 0, to: value.count, by: 2).map { index in
            let start = value.index(value.startIndex, offsetBy: index)
            let end = value.index(start, offsetBy: 2)
            return UInt8(value[start..<end], radix: 16)!
        }
    }

    private func hexEncode(_ bytes: [UInt8]) -> String {
        bytes.map { String(format: "%02x", $0) }.joined()
    }
}
