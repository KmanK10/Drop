import Foundation

public struct DropSettings: Equatable {
    public var serverURL: String
    public var username: String

    /// No server is built in. The sign-in field starts empty.
    public static let defaultServer = ""
    public static let empty = DropSettings(serverURL: defaultServer, username: "")

    public init(serverURL: String, username: String) {
        self.serverURL = serverURL
        self.username = username
    }

    public static func fileURL() -> URL {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask).first
            ?? URL(fileURLWithPath: NSTemporaryDirectory())
        return base.appendingPathComponent("Drop", isDirectory: true).appendingPathComponent("config.json")
    }

    /// Only the server address and username are kept. A password or content key
    /// in the file is ignored and removed.
    public static func load(from url: URL = fileURL()) -> DropSettings {
        guard let data = try? Data(contentsOf: url),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
        else { return .empty }
        let server = (object["serverUrl"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines)
        let username = (object["username"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines)
        let settings = DropSettings(
            serverURL: (server?.isEmpty == false && (server?.count ?? 0) < 500) ? server! : defaultServer,
            username: (username?.count ?? 99) <= 64 && username?.contains(where: { $0 == "\n" || $0 == "\r" }) != true
                ? (username ?? "")
                : ""
        )
        if Set(object.keys) != ["serverUrl", "username"] {
            try? settings.save(to: url)
        }
        return settings
    }

    public func save(to url: URL = Self.fileURL()) throws {
        try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let object: [String: String] = ["serverUrl": serverURL, "username": username]
        let data = try JSONSerialization.data(withJSONObject: object, options: [.prettyPrinted, .sortedKeys])
        try data.write(to: url, options: .atomic)
    }
}
