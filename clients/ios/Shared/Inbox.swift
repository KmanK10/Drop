import Foundation

/// Plaintext staged by the share extension until the running app encrypts it.
/// The content key is never written here.
enum DropInbox {
    static let groupID = "group.com.kiefermenard.drop"

    struct Staged {
        var kind: String
        var name: String
        var mime: String
        var bodyURL: URL
        var jsonURL: URL
    }

    static func containerURL() -> URL? {
        FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: groupID)
    }

    static func directory() throws -> URL {
        guard let base = containerURL() else {
            throw InboxError.unavailable
        }
        let dir = base.appendingPathComponent("Inbox", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    static func stage(kind: String, name: String, mime: String, body: Data) throws {
        let dir = try directory()
        let id = UUID().uuidString
        let bodyURL = dir.appendingPathComponent("\(id).body")
        let jsonURL = dir.appendingPathComponent("\(id).json")
        let bodyTmp = dir.appendingPathComponent("\(id).body.tmp")
        let jsonTmp = dir.appendingPathComponent("\(id).json.tmp")
        try body.write(to: bodyTmp, options: .atomic)
        let json = try JSONSerialization.data(withJSONObject: ["kind": kind, "name": name, "mime": mime])
        try json.write(to: jsonTmp, options: .atomic)
        try FileManager.default.moveItem(at: bodyTmp, to: bodyURL)
        try FileManager.default.moveItem(at: jsonTmp, to: jsonURL)
    }

    static func stageFile(name: String, mime: String, from source: URL) throws {
        let dir = try directory()
        let id = UUID().uuidString
        let bodyURL = dir.appendingPathComponent("\(id).body")
        let jsonURL = dir.appendingPathComponent("\(id).json")
        let jsonTmp = dir.appendingPathComponent("\(id).json.tmp")
        let accessed = source.startAccessingSecurityScopedResource()
        defer {
            if accessed { source.stopAccessingSecurityScopedResource() }
        }
        try FileManager.default.copyItem(at: source, to: bodyURL)
        let json = try JSONSerialization.data(withJSONObject: ["kind": "file", "name": name, "mime": mime])
        try json.write(to: jsonTmp, options: .atomic)
        try FileManager.default.moveItem(at: jsonTmp, to: jsonURL)
    }

    static func pending() -> [Staged] {
        guard let dir = try? directory() else { return [] }
        let names = (try? FileManager.default.contentsOfDirectory(atPath: dir.path)) ?? []
        var staged: [Staged] = []
        for name in names where name.hasSuffix(".json") {
            let id = String(name.dropLast(5))
            let jsonURL = dir.appendingPathComponent(name)
            let bodyURL = dir.appendingPathComponent(id + ".body")
            guard FileManager.default.fileExists(atPath: bodyURL.path),
                  let data = try? Data(contentsOf: jsonURL),
                  let object = try? JSONSerialization.jsonObject(with: data) as? [String: String],
                  let kind = object["kind"],
                  let itemName = object["name"],
                  let mime = object["mime"]
            else { continue }
            staged.append(Staged(kind: kind, name: itemName, mime: mime, bodyURL: bodyURL, jsonURL: jsonURL))
        }
        return staged
    }

    static func readBody(_ item: Staged) -> Data? {
        try? Data(contentsOf: item.bodyURL)
    }

    static func remove(_ item: Staged) {
        zeroFile(item.bodyURL)
        try? FileManager.default.removeItem(at: item.bodyURL)
        try? FileManager.default.removeItem(at: item.jsonURL)
    }

    private static func zeroFile(_ url: URL) {
        guard let handle = try? FileHandle(forWritingTo: url) else { return }
        defer { try? handle.close() }
        let length = (try? handle.seekToEnd()) ?? 0
        try? handle.seek(toOffset: 0)
        var remaining = length
        let chunk = Data(repeating: 0, count: 64 * 1024)
        while remaining > 0 {
            let count = Int(min(remaining, UInt64(chunk.count)))
            try? handle.write(contentsOf: chunk.prefix(count))
            remaining -= UInt64(count)
        }
    }
}

enum InboxError: Error {
    case unavailable
}
