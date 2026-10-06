import CArgon2
import CryptoKit
import Foundation
import Security

/// Same split as the browser: Argon2id yields 64 bytes. The first 32 are hashed
/// into the auth verifier. The last 32 are the AES-256-GCM content key and are
/// never sent to the server or written to disk.
enum DropCrypto {
    static let memory: UInt32 = 19_456
    static let time: UInt32 = 2
    static let parallelism: UInt32 = 1
    static let keyCheckText = "drop-key-check-v1"

    /// Upper bounds so a hostile server cannot ask the phone to allocate a huge KDF.
    /// The real parameters sit inside this range. This is not a weaker derivation.
    private static let memoryCap: UInt32 = 262_144
    private static let timeCap: UInt32 = 16
    private static let parallelismCap: UInt32 = 4

    struct Params {
        var algo: String
        var memory: UInt32
        var time: UInt32
        var parallelism: UInt32
    }

    struct Derived {
        var authVerifier: [UInt8]
        var contentKey: [UInt8]
    }

    static func normalizePassword(_ password: String) -> String {
        password.precomposedStringWithCompatibilityMapping
    }

    /// Same rules as the website. The confirmation is compared after NFKC.
    static func passwordError(_ password: String, confirm: String? = nil) -> String? {
        let normalized = normalizePassword(password)
        if normalized.count < 10 { return "Use at least 10 characters." }
        if normalized.count > 200 { return "That password is too long." }
        if let confirm, normalizePassword(confirm) != normalized {
            return "Those passwords don't match."
        }
        return nil
    }

    struct AccountMaterial {
        var salt: [UInt8]
        var authVerifier: [UInt8]
        var contentKey: [UInt8]
        var keyCheck: [UInt8]
    }

    /// A fresh salt, verifier, content key, and key check. Nothing here is written to a file.
    static func accountMaterial(password: String) throws -> AccountMaterial {
        if let problem = passwordError(password) { throw DropError.message(problem) }
        var salt = [UInt8](repeating: 0, count: 16)
        let status = SecRandomCopyBytes(kSecRandomDefault, salt.count, &salt)
        guard status == errSecSuccess else { throw DropError.message("Couldn't start the password change.") }
        let params = Params(algo: "argon2id", memory: memory, time: time, parallelism: parallelism)
        var derived = try deriveKeys(password: password, salt: salt, params: params)
        defer {
            Bytes.wipe(&derived.authVerifier)
            Bytes.wipe(&derived.contentKey)
        }
        let keyCheck = try encrypt(key: derived.contentKey, plaintext: Array(keyCheckText.utf8))
        return AccountMaterial(
            salt: salt,
            authVerifier: Array(derived.authVerifier),
            contentKey: Array(derived.contentKey),
            keyCheck: keyCheck
        )
    }

    /// Fields the server accepts for a new password. The content key is not one of them.
    static func registrationFields(salt: [UInt8], authVerifier: [UInt8], keyCheck: [UInt8]) -> [String: Any] {
        [
            "authVerifier": Bytes.b64url(authVerifier),
            "kdfSalt": Bytes.b64url(salt),
            "kdfMemory": NSNumber(value: memory),
            "kdfTime": NSNumber(value: time),
            "kdfParallelism": NSNumber(value: parallelism),
            "keyCheck": Bytes.b64url(keyCheck),
        ]
    }

    static func assertStrong(_ params: Params) throws {
        guard params.algo == "argon2id" else { throw DropError.badKdf }
        guard params.memory >= memory, params.time >= time, params.parallelism >= 1,
              params.memory <= memoryCap, params.time <= timeCap, params.parallelism <= parallelismCap
        else { throw DropError.weakKdf }
    }

    static func deriveKeys(password: String, salt: [UInt8], params: Params) throws -> Derived {
        try assertStrong(params)
        guard (16...64).contains(salt.count) else { throw DropError.badKdf }
        let normalized = normalizePassword(password)
        if normalized.count > 200 { throw DropError.longPassword }
        var passwordBytes = Array(normalized.utf8)
        var derived = [UInt8](repeating: 0, count: 64)
        let rc: Int32 = derived.withUnsafeMutableBytes { derivedBuf in
            salt.withUnsafeBytes { saltBuf in
                passwordBytes.withUnsafeBytes { passwordBuf in
                    argon2id_hash_raw(
                        params.time,
                        params.memory,
                        params.parallelism,
                        passwordBuf.baseAddress,
                        passwordBuf.count,
                        saltBuf.baseAddress,
                        saltBuf.count,
                        derivedBuf.baseAddress,
                        64
                    )
                }
            }
        }
        Bytes.wipe(&passwordBytes)
        guard rc == 0 else {
            Bytes.wipe(&derived)
            throw DropError.message("Couldn't derive the key.")
        }
        var authSecret = Array(derived[..<32])
        var contentKey = Array(derived[32...])
        Bytes.wipe(&derived)
        let verifier = Bytes.sha256(authSecret)
        Bytes.wipe(&authSecret)
        return Derived(authVerifier: verifier, contentKey: contentKey)
    }

    static func encrypt(key: [UInt8], plaintext: [UInt8]) throws -> [UInt8] {
        guard key.count == 32 else { throw DropError.message("Couldn't encrypt that.") }
        var nonceBytes = [UInt8](repeating: 0, count: 12)
        let status = SecRandomCopyBytes(kSecRandomDefault, nonceBytes.count, &nonceBytes)
        guard status == errSecSuccess else { throw DropError.message("Couldn't encrypt that.") }
        do {
            let nonce = try AES.GCM.Nonce(data: Data(nonceBytes))
            let sealed = try AES.GCM.seal(Data(plaintext), using: SymmetricKey(data: Data(key)), nonce: nonce)
            guard let combined = sealed.combined else { throw DropError.message("Couldn't encrypt that.") }
            return Array(combined)
        } catch let error as DropError {
            throw error
        } catch {
            throw DropError.message("Couldn't encrypt that.")
        }
    }

    static func decrypt(key: [UInt8], blob: [UInt8]) throws -> [UInt8] {
        guard key.count == 32, blob.count >= 12 + 16 else {
            throw DropError.message("Ciphertext is too short.")
        }
        do {
            let box = try AES.GCM.SealedBox(combined: Data(blob))
            let plain = try AES.GCM.open(box, using: SymmetricKey(data: Data(key)))
            return Array(plain)
        } catch {
            throw DropError.message("Couldn't decrypt that.")
        }
    }

    static func verifyKeyCheck(contentKey: [UInt8], blob: [UInt8]) -> Bool {
        guard let plain = try? decrypt(key: contentKey, blob: blob) else { return false }
        return Bytes.timingEqual(plain, Array(keyCheckText.utf8))
    }

    static func parseKdf(_ object: [String: Any]) throws -> (Params, [UInt8]) {
        let algo = object["algo"] as? String ?? ""
        guard let memory = jsonUInt32(object["memory"]),
              let time = jsonUInt32(object["time"]),
              let parallelism = jsonUInt32(object["parallelism"]),
              let saltText = object["salt"] as? String,
              let salt = Bytes.fromB64url(saltText)
        else { throw DropError.badKdf }
        let params = Params(algo: algo, memory: memory, time: time, parallelism: parallelism)
        try assertStrong(params)
        guard salt.count >= 16 else { throw DropError.badKdf }
        return (params, salt)
    }
}

/// Password rules shared with the website. The app uses this before calling the account API.
public enum DropPassword {
    public static func rejection(_ password: String, confirm: String?) -> String? {
        DropCrypto.passwordError(password, confirm: confirm)
    }
}

func jsonUInt32(_ value: Any?) -> UInt32? {
    if let number = value as? NSNumber {
        let wide = number.uint64Value
        guard wide <= UInt64(UInt32.max), number.doubleValue.rounded() == number.doubleValue else { return nil }
        return UInt32(wide)
    }
    return nil
}

func jsonUInt64(_ value: Any?) -> UInt64? {
    if let number = value as? NSNumber {
        guard number.doubleValue >= 0 else { return nil }
        return number.uint64Value
    }
    return nil
}
