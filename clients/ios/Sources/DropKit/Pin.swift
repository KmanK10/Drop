import CArgon2
import Foundation
import Security

/// Argon2id plus AES-256-GCM around an unlock blob. The PIN is not in the result.
public enum DropPin {
    public static func rejection(_ pin: String, confirm: String?) -> String? {
        let normalized = DropCrypto.normalizePassword(pin)
        if !isDigits(normalized) {
            return "Use 4 to 8 digits."
        }
        if let confirm, DropCrypto.normalizePassword(confirm) != normalized {
            return "Those PINs don't match."
        }
        return nil
    }

    public static func wrap(pin: String, secret: [UInt8]) throws -> [UInt8] {
        if rejection(pin, confirm: nil) != nil {
            throw DropError.message("Use 4 to 8 digits.")
        }
        var salt = [UInt8](repeating: 0, count: 16)
        let status = SecRandomCopyBytes(kSecRandomDefault, salt.count, &salt)
        guard status == errSecSuccess else { throw DropError.message("Couldn't store the PIN.") }
        var key = try pinKey(pin: pin, salt: salt)
        defer { Bytes.wipe(&key) }
        let sealed = try DropCrypto.encrypt(key: key, plaintext: secret)
        var out = [UInt8]()
        out.append(contentsOf: Array("DRPP".utf8))
        out.append(1)
        out.append(contentsOf: salt)
        out.append(contentsOf: sealed)
        return out
    }

    /// A wrong PIN fails the cipher. The caller leaves the stored blob alone.
    public static func unwrap(pin: String, blob: [UInt8]) throws -> [UInt8] {
        let magic = Array("DRPP".utf8)
        guard blob.count >= 4 + 1 + 16 + 12 + 16, Array(blob.prefix(4)) == magic, blob[4] == 1 else {
            throw DropError.message("That PIN is wrong.")
        }
        let salt = Array(blob[5..<21])
        let sealed = Array(blob[21...])
        var key = try pinKey(pin: pin, salt: salt)
        defer { Bytes.wipe(&key) }
        do {
            return try DropCrypto.decrypt(key: key, blob: sealed)
        } catch {
            throw DropError.message("That PIN is wrong.")
        }
    }

    private static func isDigits(_ value: String) -> Bool {
        let scalars = Array(value.unicodeScalars)
        return (4...8).contains(scalars.count) && scalars.allSatisfy { (48...57).contains($0.value) }
    }

    private static func pinKey(pin: String, salt: [UInt8]) throws -> [UInt8] {
        guard salt.count == 16 else { throw DropError.message("That PIN is wrong.") }
        let normalized = DropCrypto.normalizePassword(pin)
        var passwordBytes = Array(normalized.utf8)
        var derived = [UInt8](repeating: 0, count: 32)
        let rc: Int32 = derived.withUnsafeMutableBytes { derivedBuf in
            salt.withUnsafeBytes { saltBuf in
                passwordBytes.withUnsafeBytes { passwordBuf in
                    argon2id_hash_raw(
                        DropCrypto.time,
                        DropCrypto.memory,
                        DropCrypto.parallelism,
                        passwordBuf.baseAddress,
                        passwordBuf.count,
                        saltBuf.baseAddress,
                        saltBuf.count,
                        derivedBuf.baseAddress,
                        32
                    )
                }
            }
        }
        Bytes.wipe(&passwordBytes)
        guard rc == 0 else {
            Bytes.wipe(&derived)
            throw DropError.message("Couldn't derive the PIN key.")
        }
        return derived
    }
}
