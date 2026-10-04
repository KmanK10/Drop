import Foundation

public struct DropAccount: Equatable, Sendable {
    public var username: String
    public var role: String
    public var quotaBytes: UInt64
    public var usedBytes: UInt64
    public var ttlMs: UInt64
}

public struct DropItem: Equatable, Sendable, Identifiable {
    public var id: String
    public var createdAt: Int64
    public var size: UInt64
    public var kind: String
    public var title: String
    public var detail: String
    public var canCopy: Bool
    public var broken: Bool
}

public struct DropSnapshot: Sendable {
    public var account: DropAccount
    public var items: [DropItem]
    public var http: Bool
}

public enum DropCopy: Sendable {
    case text(String)
    case image(Data)
}

public struct DropDownload: Sendable {
    public var name: String
    public var bytes: Data
}

/// Talks to an existing Drop server. The session cookie lives in an ephemeral
/// URLSession. The content key lives in this object until `signOut()` or the
/// process ends. Neither is written to disk, Keychain, or UserDefaults.
public final class DropClient: @unchecked Sendable {
    private let lock = NSLock()
    private let session: URLSession
    private let redirector = RedirectStop()
    private var base: URL
    private var contentKey: [UInt8]?
    private var account: DropAccount?
    private var items: [MemoryItem] = []

    public init(server: String) throws {
        self.base = try DropClient.parseServer(server)
        let configuration = URLSessionConfiguration.ephemeral
        configuration.httpCookieAcceptPolicy = .always
        configuration.httpShouldSetCookies = true
        configuration.urlCache = nil
        configuration.requestCachePolicy = .reloadIgnoringLocalCacheData
        configuration.timeoutIntervalForRequest = 600
        configuration.timeoutIntervalForResource = 600
        configuration.httpAdditionalHeaders = ["User-Agent": "Drop"]
        let queue = OperationQueue()
        queue.maxConcurrentOperationCount = 1
        let session = URLSession(configuration: configuration, delegate: redirector, delegateQueue: queue)
        self.session = session
    }

    deinit {
        wipe()
        session.configuration.httpCookieStorage?.removeCookies(since: Date.distantPast)
        session.invalidateAndCancel()
    }

    public var isUnlocked: Bool {
        lock.lock()
        defer { lock.unlock() }
        return contentKey != nil
    }

    public func signIn(username: String, password: String) throws -> DropSnapshot {
        let name = try normalizeUsername(username)
        if password.isEmpty { throw DropError.emptyPassword }
        if DropCrypto.normalizePassword(password).count > 200 { throw DropError.longPassword }
        wipe()
        clearCookies()
        let (params, salt) = try authParams(username: name)
        var derived = try DropCrypto.deriveKeys(password: password, salt: salt, params: params)
        defer { Bytes.wipe(&derived.authVerifier); Bytes.wipe(&derived.contentKey) }
        let login = try postJSON(
            "/api/auth/login",
            ["username": name, "authVerifier": Bytes.b64url(derived.authVerifier)],
            session: false
        )
        _ = login
        let me: [String: Any]
        do {
            me = try getJSON("/api/me")
        } catch DropError.signedOut {
            throw DropError.sessionLost
        } catch {
            logoutRequest()
            clearCookies()
            throw error
        }
        guard let kdf = me["kdf"] as? [String: Any] else { throw DropError.keyCheck }
        let (meParams, meSalt) = try DropCrypto.parseKdf(kdf)
        guard meParams.algo == params.algo, meParams.memory == params.memory, meParams.time == params.time,
              meParams.parallelism == params.parallelism, meSalt == salt
        else {
            logoutRequest()
            clearCookies()
            throw DropError.keyCheck
        }
        guard let checkText = me["keyCheck"] as? String, let check = Bytes.fromB64url(checkText),
              DropCrypto.verifyKeyCheck(contentKey: derived.contentKey, blob: check)
        else {
            logoutRequest()
            clearCookies()
            throw DropError.keyCheck
        }
        let ttl = fetchTTL()
        lock.lock()
        contentKey = derived.contentKey.map { $0 }
        account = accountFrom(me, ttl: ttl)
        lock.unlock()
        Bytes.wipe(&derived.contentKey)
        return try refresh()
    }

    public func signOut() {
        wipe()
        logoutRequest()
        clearCookies()
    }

    public func refresh() throws -> DropSnapshot {
        try ensureUnlocked()
        let payload: [String: Any]
        do {
            payload = try getJSON("/api/items")
        } catch DropError.signedOut {
            wipe()
            clearCookies()
            throw DropError.signedOut
        }
        lock.lock()
        if var current = account {
            current.usedBytes = jsonUInt64(payload["usedBytes"]) ?? 0
            if let quota = jsonUInt64(payload["quotaBytes"]), quota > 0 {
                current.quotaBytes = quota
            }
            account = current
        }
        let rows = payload["items"] as? [[String: Any]] ?? []
        var next: [MemoryItem] = []
        lock.unlock()
        for row in rows {
            let id = row["id"] as? String ?? ""
            guard validID(id) else { continue }
            let createdAt = (row["createdAt"] as? NSNumber)?.int64Value ?? 0
            let size = jsonUInt64(row["size"]) ?? 0
            lock.lock()
            let existing = items.first { $0.summary.id == id }
            lock.unlock()
            if var existing {
                existing.summary.createdAt = createdAt
                existing.summary.size = size
                if existing.summary.kind != "text" && !existing.summary.broken {
                    existing.summary.detail = DropFormat.bytes(size)
                }
                next.append(existing)
                continue
            }
            do {
                next.append(try decryptNew(id: id, createdAt: createdAt, size: size))
            } catch DropError.signedOut {
                wipe()
                clearCookies()
                throw DropError.signedOut
            } catch DropError.missing {
                continue
            } catch {
                next.append(broken(id: id, createdAt: createdAt, size: size))
            }
        }
        lock.lock()
        items = next
        let snapshot: DropSnapshot
        do {
            snapshot = try snapshotLocked()
        } catch {
            lock.unlock()
            throw error
        }
        lock.unlock()
        return snapshot
    }

    public func uploadText(_ text: String) throws -> DropSnapshot {
        if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { throw DropError.emptyText }
        return try uploadPlain(ItemPlain(kind: .text, name: "", mime: "text/plain", body: Array(text.utf8)))
    }

    public func uploadFile(name: String, mime: String, bytes: [UInt8]) throws -> DropSnapshot {
        let safeName = name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "file" : name
        let safeMime = mime.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "application/octet-stream" : mime
        return try uploadPlain(ItemPlain(kind: .file, name: safeName, mime: safeMime, body: bytes))
    }

    public func deleteItem(_ id: String) throws -> DropSnapshot {
        try ensureUnlocked()
        let path = try itemPath(id)
        let (status, bytes) = try request(method: "DELETE", path: path, body: nil, contentType: nil)
        if status == 401 {
            wipe()
            clearCookies()
            throw DropError.signedOut
        }
        guard (200..<300).contains(status) else { throw errorFrom(status, bytes) }
        lock.lock()
        items.removeAll { $0.summary.id == id }
        lock.unlock()
        return try refresh()
    }

    public func copyItem(_ id: String) throws -> DropCopy {
        try ensureUnlocked()
        lock.lock()
        let existing = items.first { $0.summary.id == id }
        lock.unlock()
        if let existing {
            if existing.summary.broken { throw DropError.cantCopy }
            if existing.summary.kind == "text" {
                return .text(existing.text ?? "")
            }
        }
        var plain = try fetchPlain(id)
        defer { Bytes.wipe(&plain.body) }
        switch plain.kind {
        case .text:
            return .text(String(decoding: plain.body, as: UTF8.self))
        case .file:
            switch ClipboardKinds.fileKind(plain.mime) {
            case .text, .html:
                return .text(String(decoding: plain.body, as: UTF8.self))
            case .image:
                return .image(Data(plain.body))
            case nil:
                throw DropError.cantCopy
            }
        }
    }

    public func downloadItem(_ id: String) throws -> DropDownload {
        try ensureUnlocked()
        var plain = try fetchPlain(id)
        defer { Bytes.wipe(&plain.body) }
        let name: String
        switch plain.kind {
        case .text:
            let named = ItemCodec.safeDownloadName(plain.name)
            if named == "download" {
                name = "drop.txt"
            } else if named.contains(".") {
                name = named
            } else {
                name = named + ".txt"
            }
        case .file:
            name = ItemCodec.safeDownloadName(plain.name)
        }
        return DropDownload(name: name, bytes: Data(plain.body))
    }

    public static func parseServer(_ input: String) throws -> URL {
        var withoutTrail = input.trimmingCharacters(in: .whitespacesAndNewlines)
        while withoutTrail.hasSuffix("/") {
            withoutTrail.removeLast()
        }
        guard let components = URLComponents(string: withoutTrail),
              let scheme = components.scheme?.lowercased(),
              scheme == "https" || scheme == "http",
              let host = components.host, !host.isEmpty
        else { throw DropError.badServer }
        if components.user != nil || components.password != nil {
            throw DropError.message("Don't put a username or password in the server address.")
        }
        let path = components.path
        if !path.isEmpty && path != "/" {
            throw DropError.message("The server address shouldn't include a path.")
        }
        if components.query != nil || components.fragment != nil {
            throw DropError.badServer
        }
        guard let url = components.url else { throw DropError.badServer }
        return url
    }

    public static func normalizeUsername(_ value: String) throws -> String {
        guard let name = normalizeUsernameOptional(value) else { throw DropError.badUsername }
        return name
    }

    private func uploadPlain(_ plain: ItemPlain) throws -> DropSnapshot {
        var key = try ensureUnlocked()
        defer { Bytes.wipe(&key) }
        lock.lock()
        let current = account
        lock.unlock()
        guard let current else { throw DropError.locked }
        var body = plain.body
        var encoded = try ItemCodec.encode(plain)
        Bytes.wipe(&body)
        let overhead = UInt64(encoded.count) + 32
        let sum = current.usedBytes.addingReportingOverflow(overhead)
        if sum.overflow || sum.partialValue > current.quotaBytes {
            Bytes.wipe(&encoded)
            throw DropError.server("That item doesn't fit. \(DropFormat.bytes(current.usedBytes)) of \(DropFormat.bytes(current.quotaBytes)) is already used.")
        }
        let ciphertext: [UInt8]
        do {
            ciphertext = try DropCrypto.encrypt(key: key, plaintext: encoded)
        } catch {
            Bytes.wipe(&encoded)
            throw error
        }
        Bytes.wipe(&encoded)
        let (status, response) = try request(
            method: "POST",
            path: "/api/items",
            body: Data(ciphertext),
            contentType: "application/octet-stream"
        )
        if status == 401 {
            wipe()
            clearCookies()
            throw DropError.signedOut
        }
        guard (200..<300).contains(status) else { throw errorFrom(status, response) }
        return try refresh()
    }

    private func decryptNew(id: String, createdAt: Int64, size: UInt64) throws -> MemoryItem {
        var plain = try fetchPlain(id)
        let summary = summarize(plain, size: size)
        Bytes.wipe(&plain.body)
        return MemoryItem(summary: DropItem(
            id: id,
            createdAt: createdAt,
            size: size,
            kind: plain.kind.rawValue,
            title: summary.title,
            detail: summary.detail,
            canCopy: summary.canCopy,
            broken: false
        ), text: summary.text)
    }

    private func fetchPlain(_ id: String) throws -> ItemPlain {
        var key = try ensureUnlocked()
        defer { Bytes.wipe(&key) }
        let path = try itemPath(id)
        let (status, bytes) = try request(method: "GET", path: path, body: nil, contentType: nil)
        if status == 401 { throw DropError.signedOut }
        if status == 404 { throw DropError.missing }
        guard (200..<300).contains(status) else { throw errorFrom(status, bytes) }
        var plainBytes = try DropCrypto.decrypt(key: key, blob: Array(bytes))
        let item = try ItemCodec.decode(plainBytes)
        Bytes.wipe(&plainBytes)
        return item
    }

    private func authParams(username: String) throws -> (DropCrypto.Params, [UInt8]) {
        let body = try postJSON("/api/auth/params", ["username": username], session: false)
        return try DropCrypto.parseKdf(body)
    }

    private func fetchTTL() -> UInt64 {
        if let meta = try? getJSON("/api/meta"), let ttl = jsonUInt64(meta["itemTtlMs"]), ttl > 0 {
            return ttl
        }
        return 30 * 24 * 60 * 60 * 1000
    }

    private func logoutRequest() {
        var request = URLRequest(url: base.appendingPathComponent("api/auth/logout"))
        request.httpMethod = "POST"
        request.timeoutInterval = 3
        request.setValue("1", forHTTPHeaderField: "x-drop-request")
        let done = DispatchSemaphore(value: 0)
        session.dataTask(with: request) { _, _, _ in done.signal() }.resume()
        _ = done.wait(timeout: .now() + 4)
    }

    private func ensureUnlocked() throws -> [UInt8] {
        lock.lock()
        defer { lock.unlock() }
        guard let contentKey else { throw DropError.locked }
        return contentKey.map { $0 }
    }

    private func wipe() {
        lock.lock()
        if var key = contentKey {
            Bytes.wipe(&key)
        }
        contentKey = nil
        items.removeAll()
        account = nil
        lock.unlock()
    }

    private func clearCookies() {
        session.configuration.httpCookieStorage?.removeCookies(since: Date.distantPast)
    }

    private func snapshotLocked() throws -> DropSnapshot {
        guard let account else { throw DropError.locked }
        return DropSnapshot(account: account, items: items.map(\.summary), http: base.scheme == "http")
    }

    private func postJSON(_ path: String, _ body: [String: Any], session expected: Bool) throws -> [String: Any] {
        let data = try JSONSerialization.data(withJSONObject: body)
        let (status, response) = try request(method: "POST", path: path, body: data, contentType: "application/json")
        return try interpret(status, response, session: expected)
    }

    private func getJSON(_ path: String) throws -> [String: Any] {
        let (status, response) = try request(method: "GET", path: path, body: nil, contentType: nil)
        return try interpret(status, response, session: true)
    }

    private func request(method: String, path: String, body: Data?, contentType: String?) throws -> (Int, Data) {
        let url = try absolute(path)
        var request = URLRequest(url: url)
        request.httpMethod = method
        request.timeoutInterval = 600
        request.setValue("application/json", forHTTPHeaderField: "Accept")
        if method != "GET" && method != "HEAD" {
            request.setValue("1", forHTTPHeaderField: "x-drop-request")
        }
        if let contentType {
            request.setValue(contentType, forHTTPHeaderField: "Content-Type")
        }
        request.httpBody = body
        let done = DispatchSemaphore(value: 0)
        var outcome: Result<(Int, Data), Error>?
        session.dataTask(with: request) { data, response, error in
            defer { done.signal() }
            if error != nil {
                outcome = .failure(DropError.network)
                return
            }
            let status = (response as? HTTPURLResponse)?.statusCode ?? 0
            outcome = .success((status, data ?? Data()))
        }.resume()
        done.wait()
        switch outcome {
        case .success(let value):
            return value
        case .failure(let error):
            throw error
        case nil:
            throw DropError.network
        }
    }

    private func absolute(_ path: String) throws -> URL {
        var root = base.absoluteString
        while root.hasSuffix("/") {
            root.removeLast()
        }
        let suffix = path.hasPrefix("/") ? path : "/" + path
        guard let url = URL(string: root + suffix) else { throw DropError.badServer }
        return url
    }

    private func interpret(_ status: Int, _ bytes: Data, session expected: Bool) throws -> [String: Any] {
        if expected && status == 401 { throw DropError.signedOut }
        guard (200..<300).contains(status) else { throw errorFrom(status, bytes) }
        guard let object = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any] else {
            throw DropError.message("Something went wrong.")
        }
        return object
    }

    private func errorFrom(_ status: Int, _ bytes: Data) -> DropError {
        if let object = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any],
           let message = object["error"] as? String,
           !message.isEmpty, message.count < 400, !message.contains("\n"), !message.contains("\r") {
            return .server(message)
        }
        if status == 413 { return .server("That item is too large.") }
        return .server("Something went wrong.")
    }
}

private struct MemoryItem {
    var summary: DropItem
    var text: String?
}

private final class RedirectStop: NSObject, URLSessionTaskDelegate {
    func urlSession(
        _ session: URLSession,
        task: URLSessionTask,
        willPerformHTTPRedirection response: HTTPURLResponse,
        newRequest request: URLRequest,
        completionHandler: @escaping (URLRequest?) -> Void
    ) {
        completionHandler(nil)
    }
}

private func normalizeUsernameOptional(_ value: String) -> String? {
    let name = value.trimmingCharacters(in: .whitespacesAndNewlines)
        .precomposedStringWithCompatibilityMapping
        .lowercased(with: Locale(identifier: "en_US_POSIX"))
    guard let first = name.first, first.isASCII && first.isLetter && first.isLowercase else { return nil }
    guard (2...32).contains(name.count) else { return nil }
    guard name.allSatisfy({ $0.isASCII && ($0.isLowercase || $0.isNumber || $0 == "_" || $0 == "-") }) else { return nil }
    return name
}

private func accountFrom(_ me: [String: Any], ttl: UInt64) -> DropAccount {
    DropAccount(
        username: me["username"] as? String ?? "",
        role: me["role"] as? String ?? "user",
        quotaBytes: jsonUInt64(me["quotaBytes"]) ?? 0,
        usedBytes: jsonUInt64(me["usedBytes"]) ?? 0,
        ttlMs: ttl
    )
}

private func summarize(_ plain: ItemPlain, size: UInt64) -> (title: String, detail: String, canCopy: Bool, text: String?) {
    switch plain.kind {
    case .text:
        let text = String(decoding: plain.body, as: UTF8.self)
        let preview = DropFormat.preview(text, limit: 80)
        return (preview.isEmpty ? "Empty note" : preview, "Text · \(DropFormat.bytes(UInt64(text.utf8.count)))", true, text)
    case .file:
        let title = plain.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "Untitled file" : plain.name
        return (title, DropFormat.bytes(size), ClipboardKinds.fileKind(plain.mime) != nil, nil)
    }
}

private func broken(id: String, createdAt: Int64, size: UInt64) -> MemoryItem {
    MemoryItem(
        summary: DropItem(
            id: id,
            createdAt: createdAt,
            size: size,
            kind: "file",
            title: "Can't decrypt this item",
            detail: DropFormat.bytes(size),
            canCopy: false,
            broken: true
        ),
        text: nil
    )
}

private func validID(_ id: String) -> Bool {
    id.count == 36 && id.allSatisfy { ch in
        ch == "-" || ("0"..."9").contains(ch) || ("a"..."f").contains(ch) || ("A"..."F").contains(ch)
    }
}

private func itemPath(_ id: String) throws -> String {
    guard validID(id) else { throw DropError.missing }
    return "/api/items/\(id)"
}

public func mimeForFilename(_ name: String) -> String {
    switch (name as NSString).pathExtension.lowercased() {
    case "png": return "image/png"
    case "jpg", "jpeg": return "image/jpeg"
    case "gif": return "image/gif"
    case "webp": return "image/webp"
    case "svg": return "image/svg+xml"
    case "txt": return "text/plain"
    case "md": return "text/markdown"
    case "json": return "application/json"
    case "pdf": return "application/pdf"
    case "zip": return "application/zip"
    case "html", "htm": return "text/html"
    case "csv": return "text/csv"
    case "xml": return "application/xml"
    case "mp4": return "video/mp4"
    case "mov": return "video/quicktime"
    case "mp3": return "audio/mpeg"
    case "wav": return "audio/wav"
    case "heic": return "image/heic"
    default: return "application/octet-stream"
    }
}
