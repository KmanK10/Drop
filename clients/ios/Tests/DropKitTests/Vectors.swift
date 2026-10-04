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

    func testPasswordRulesMatchTheBrowser() {
        XCTAssertEqual(DropCrypto.passwordError("short", confirm: "short"), "Use at least 10 characters.")
        let tooLong = String(repeating: "a", count: 201)
        XCTAssertEqual(DropCrypto.passwordError(tooLong, confirm: tooLong), "That password is too long.")
        XCTAssertEqual(
            DropCrypto.passwordError("long-enough-password", confirm: "different-password"),
            "Those passwords don't match."
        )
        XCTAssertNil(DropCrypto.passwordError("long-enough-password", confirm: "long-enough-password"))
        XCTAssertNil(DropCrypto.passwordError("password\u{FF11}ok", confirm: "password1ok"))
        XCTAssertEqual(DropPassword.rejection("short", confirm: "short"), "Use at least 10 characters.")
    }

    func testItemAgeMatchesTheMacWording() {
        let now: Int64 = 1_700_000_000_000
        XCTAssertEqual(DropFormat.when(now, nowMs: now), "just now")
        XCTAssertEqual(DropFormat.when(now - 14_000, nowMs: now), "just now")
        XCTAssertEqual(DropFormat.when(now - 14_499, nowMs: now), "just now")
        XCTAssertEqual(DropFormat.when(now - 14_500, nowMs: now), "15s ago")
        XCTAssertEqual(DropFormat.when(now - 15_000, nowMs: now), "15s ago")
        XCTAssertEqual(DropFormat.when(now - 59_000, nowMs: now), "59s ago")
        XCTAssertEqual(DropFormat.when(now - 60_000, nowMs: now), "1m ago")
        XCTAssertEqual(DropFormat.when(now - 90_000, nowMs: now), "2m ago")
        XCTAssertEqual(DropFormat.when(now - 59 * 60_000, nowMs: now), "59m ago")
        XCTAssertEqual(DropFormat.when(now - 60 * 60_000, nowMs: now), "1h ago")
        XCTAssertEqual(DropFormat.when(now - 23 * 60 * 60_000, nowMs: now), "23h ago")
        XCTAssertEqual(DropFormat.when(now - 24 * 60 * 60_000, nowMs: now), "1d ago")
        XCTAssertEqual(DropFormat.when(now - 6 * 24 * 60 * 60_000, nowMs: now), "6d ago")
        XCTAssertEqual(DropFormat.when(now - 7 * 24 * 60 * 60_000, nowMs: now), "Nov 7")
        XCTAssertEqual(DropFormat.when(now - 8 * 24 * 60 * 60_000, nowMs: now), "Nov 6")
        XCTAssertEqual(DropFormat.when(now - 40 * 24 * 60 * 60_000, nowMs: now), "Oct 5")
        XCTAssertEqual(DropFormat.when(1_669_852_800_000, nowMs: now), "Dec 1, 2022")
    }

    func testDaysLeftWarnsOnlyUnderFourDays() {
        let day: Int64 = 86_400_000
        let ttl = UInt64(30 * day)
        let created: Int64 = 1_000_000_000_000
        let expires = created + 30 * day
        XCTAssertNil(DropFormat.daysLeft(createdAt: created, ttlMs: ttl, nowMs: expires - 4 * day))
        XCTAssertEqual(DropFormat.daysLeft(createdAt: created, ttlMs: ttl, nowMs: expires - 4 * day + 1), "3 days left")
        XCTAssertEqual(DropFormat.daysLeft(createdAt: created, ttlMs: ttl, nowMs: expires - 3 * day), "3 days left")
        XCTAssertEqual(DropFormat.daysLeft(createdAt: created, ttlMs: ttl, nowMs: expires - 2 * day), "2 days left")
        XCTAssertEqual(DropFormat.daysLeft(createdAt: created, ttlMs: ttl, nowMs: expires - day), "1 day left")
        XCTAssertEqual(DropFormat.daysLeft(createdAt: created, ttlMs: ttl, nowMs: expires - day + 1), "Less than a day")
        XCTAssertEqual(
            DropFormat.daysLeft(createdAt: created, ttlMs: ttl, nowMs: expires - 12 * 60 * 60 * 1000),
            "Less than a day"
        )
        XCTAssertEqual(DropFormat.daysLeft(createdAt: created, ttlMs: ttl, nowMs: expires), "0 days left")
        XCTAssertEqual(DropFormat.daysLeft(createdAt: created, ttlMs: ttl, nowMs: expires + 1), "0 days left")
        XCTAssertNil(DropFormat.daysLeft(createdAt: created, ttlMs: 0, nowMs: expires - day))
    }

    func testPinRulesAreFourToEightDigits() throws {
        XCTAssertEqual(DropPin.rejection("123", confirm: nil), "Use 4 to 8 digits.")
        XCTAssertEqual(DropPin.rejection("123456789", confirm: nil), "Use 4 to 8 digits.")
        XCTAssertEqual(DropPin.rejection("12ab", confirm: nil), "Use 4 to 8 digits.")
        XCTAssertEqual(DropPin.rejection("1234", confirm: "9999"), "Those PINs don't match.")
        XCTAssertNil(DropPin.rejection("1234", confirm: "1234"))
        XCTAssertNil(DropPin.rejection("\u{FF11}\u{FF12}\u{FF13}\u{FF14}", confirm: "1234"))
        let secret = Array("pin-secret-marker-not-the-password".utf8)
        let wrapped = try DropPin.wrap(pin: "1234", secret: secret)
        XCTAssertEqual(Array(wrapped.prefix(4)), Array("DRPP".utf8))
        XCTAssertFalse(containsBytes(wrapped, Array("1234".utf8)))
        XCTAssertFalse(containsBytes(wrapped, secret))
        XCTAssertEqual(try DropPin.unwrap(pin: "1234", blob: wrapped), secret)
        XCTAssertThrowsError(try DropPin.unwrap(pin: "9999", blob: wrapped))
    }

    func testRegistrationFieldsOmitTheContentKey() throws {
        let salt = [UInt8](repeating: 1, count: 16)
        let verifier = [UInt8](repeating: 2, count: 32)
        let contentKey = [UInt8](repeating: 3, count: 32)
        let keyCheck = [UInt8](repeating: 4, count: 40)
        let fields = DropCrypto.registrationFields(salt: salt, authVerifier: verifier, keyCheck: keyCheck)
        XCTAssertEqual((fields["kdfMemory"] as? NSNumber)?.intValue, 19_456)
        XCTAssertEqual((fields["kdfTime"] as? NSNumber)?.intValue, 2)
        XCTAssertEqual((fields["kdfParallelism"] as? NSNumber)?.intValue, 1)
        XCTAssertEqual(fields["authVerifier"] as? String, Bytes.b64url(verifier))
        XCTAssertEqual(fields["kdfSalt"] as? String, Bytes.b64url(salt))
        XCTAssertEqual(fields["keyCheck"] as? String, Bytes.b64url(keyCheck))
        XCTAssertNil(fields["password"])
        XCTAssertNil(fields["contentKey"])
        let encoded = String(decoding: try JSONSerialization.data(withJSONObject: fields), as: UTF8.self)
        XCTAssertFalse(encoded.contains(Bytes.b64url(contentKey)))
        XCTAssertFalse(encoded.contains("password"))
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

    func testTextSnippetStaysShortAndDropsTags() {
        XCTAssertEqual(ItemPreview.textSnippet(Array("hi".utf8)), "hi")
        let html = Array("<p>hello</p>".utf8)
        XCTAssertEqual(ItemPreview.textSnippet(html), "hello")
        let huge = Array(String(repeating: "hello ", count: 5000).utf8)
        let snippet = ItemPreview.textSnippet(huge, limit: 160)
        XCTAssertLessThanOrEqual(snippet.count, 161)
        XCTAssertTrue(snippet.hasSuffix("…"))
        XCTAssertLessThan(snippet.count, huge.count)
    }

    func testUnlockBlobRoundTripsWithoutThePassword() throws {
        let key = [UInt8](repeating: 7, count: 32)
        let blob = UnlockBlob(contentKey: key, server: "https://drop.example", username: "ada", cookie: "session-token")
        let data = try blob.encode()
        let decoded = try UnlockBlob.decode(data)
        XCTAssertEqual(decoded, blob)
        XCTAssertEqual(Array(data.prefix(4)), Array("DRPK".utf8))
        XCTAssertFalse(String(decoding: data, as: UTF8.self).contains("password"))
        XCTAssertThrowsError(try UnlockBlob.decode(data.dropLast()))
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

    private func containsBytes(_ haystack: [UInt8], _ needle: [UInt8]) -> Bool {
        guard !needle.isEmpty, needle.count <= haystack.count else { return false }
        for start in 0...(haystack.count - needle.count) {
            if Array(haystack[start..<(start + needle.count)]) == needle {
                return true
            }
        }
        return false
    }

    private func hexEncode(_ bytes: [UInt8]) -> String {
        bytes.map { String(format: "%02x", $0) }.joined()
    }
}
