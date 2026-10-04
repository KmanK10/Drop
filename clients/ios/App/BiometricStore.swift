import Foundation
import LocalAuthentication
import Security

enum BiometricKind {
    case faceID
    case touchID
    case opticID
    case none

    var label: String {
        switch self {
        case .faceID: return "Unlock with Face ID"
        case .touchID: return "Unlock with Touch ID"
        case .opticID: return "Unlock with Optic ID"
        case .none: return "Unlock with biometrics"
        }
    }
}

enum BiometricLoad {
    case success(Data)
    case canceled
    case failed(String)
}

enum BiometricSave {
    case success
    case canceled
    case failed(String)
}

/// The content-key blob lives in the app keychain, gated by Face ID or Touch ID.
/// It is not in the app-group container, and it is not a file.
enum BiometricStore {
    private static let service = "com.kiefermenard.drop"
    private static let account = "unlock"

    static func kind() -> BiometricKind {
        let context = LAContext()
        var error: NSError?
        guard context.canEvaluatePolicy(.deviceOwnerAuthenticationWithBiometrics, error: &error) else {
            return .none
        }
        switch context.biometryType {
        case .faceID: return .faceID
        case .touchID: return .touchID
        case .opticID: return .opticID
        default: return .none
        }
    }

    static func enrolled() -> Bool {
        var query = baseQuery()
        query[kSecUseAuthenticationUI as String] = kSecUseAuthenticationUIFail
        let status = SecItemCopyMatching(query as CFDictionary, nil)
        return status == errSecSuccess || status == errSecInteractionNotAllowed
    }

    static func save(_ data: Data) -> BiometricSave {
        guard kind() != .none else {
            return .failed("Biometrics aren't available on this phone.")
        }
        guard let access = SecAccessControlCreateWithFlags(
            nil,
            kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
            .biometryCurrentSet,
            nil
        ) else {
            return .failed("Couldn't store biometric unlock.")
        }
        var query = baseQuery()
        query[kSecValueData as String] = data
        query[kSecAttrAccessControl as String] = access
        switch SecItemAdd(query as CFDictionary, nil) {
        case errSecSuccess:
            return .success
        case errSecUserCanceled:
            return .canceled
        case errSecDuplicateItem:
            return replace(data)
        default:
            return .failed("Couldn't store biometric unlock.")
        }
    }

    /// Replace an existing item. Cancel leaves that item in place.
    private static func replace(_ data: Data) -> BiometricSave {
        let context = LAContext()
        context.localizedReason = "Unlock Drop"
        var query = baseQuery()
        query[kSecUseAuthenticationContext as String] = context
        let changes = [kSecValueData as String: data] as CFDictionary
        switch SecItemUpdate(query as CFDictionary, changes) {
        case errSecSuccess:
            return .success
        case errSecUserCanceled:
            return .canceled
        default:
            return .failed("Couldn't store biometric unlock.")
        }
    }

    static func load() -> BiometricLoad {
        let context = LAContext()
        context.localizedReason = "Unlock Drop"
        var query = baseQuery()
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        query[kSecUseAuthenticationContext as String] = context
        var item: CFTypeRef?
        let status = SecItemCopyMatching(query as CFDictionary, &item)
        if status == errSecUserCanceled {
            return .canceled
        }
        guard status == errSecSuccess, let item else {
            return .failed("Biometrics didn't unlock Drop. Enter your password.")
        }
        let data = item as? Data ?? Data()
        guard !data.isEmpty else {
            return .failed("Biometrics didn't unlock Drop. Enter your password.")
        }
        return .success(data)
    }

    static func delete() {
        SecItemDelete(baseQuery() as CFDictionary)
    }

    private static func baseQuery() -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }
}

func wipe(_ data: inout Data) {
    data.resetBytes(in: 0..<data.count)
    data.removeAll()
}

/// PIN wrap in the app keychain. No biometry flag, and not the app-group item.
enum PinStore {
    private static let service = "com.kiefermenard.drop.pin"
    private static let account = "unlock"

    static func enrolled() -> Bool {
        var query = baseQuery()
        query[kSecReturnAttributes as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        return SecItemCopyMatching(query as CFDictionary, nil) == errSecSuccess
    }

    static func save(_ data: Data) -> Bool {
        var add = baseQuery()
        add[kSecValueData as String] = data
        add[kSecAttrAccessible as String] = kSecAttrAccessibleWhenUnlockedThisDeviceOnly
        switch SecItemAdd(add as CFDictionary, nil) {
        case errSecSuccess:
            return true
        case errSecDuplicateItem:
            let changes = [
                kSecValueData as String: data,
                kSecAttrAccessible as String: kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
            ] as CFDictionary
            return SecItemUpdate(baseQuery() as CFDictionary, changes) == errSecSuccess
        default:
            return false
        }
    }

    static func load() -> Data? {
        var query = baseQuery()
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess, let item else {
            return nil
        }
        let data = item as? Data ?? Data()
        return data.isEmpty ? nil : data
    }

    static func delete() {
        SecItemDelete(baseQuery() as CFDictionary)
    }

    private static func baseQuery() -> [String: Any] {
        [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ]
    }
}
