import UIKit
import UniformTypeIdentifiers

/// Stages shared files for the running Drop app. This process does not have the
/// content key and does not talk to the server.
final class ShareViewController: UIViewController {
    private let label = UILabel()

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = UIColor(red: 0.969, green: 0.953, blue: 0.918, alpha: 1)
        label.text = "Adding to Drop…"
        label.textAlignment = .center
        label.numberOfLines = 0
        label.font = .preferredFont(forTextStyle: .headline)
        label.textColor = UIColor(red: 0.110, green: 0.098, blue: 0.082, alpha: 1)
        label.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(label)
        NSLayoutConstraint.activate([
            label.leadingAnchor.constraint(equalTo: view.leadingAnchor, constant: 24),
            label.trailingAnchor.constraint(equalTo: view.trailingAnchor, constant: -24),
            label.centerYAnchor.constraint(equalTo: view.centerYAnchor),
        ])
        process()
    }

    private func process() {
        let items = extensionContext?.inputItems as? [NSExtensionItem] ?? []
        let attachments = items.flatMap { $0.attachments ?? [] }
        guard !attachments.isEmpty else {
            finish("Nothing to add.")
            return
        }
        let group = DispatchGroup()
        let lock = NSLock()
        var stored = 0
        var failures = 0
        for attachment in attachments {
            group.enter()
            stage(attachment) { result in
                lock.lock()
                switch result {
                case .success: stored += 1
                case .failure: failures += 1
                }
                lock.unlock()
                group.leave()
            }
        }
        group.notify(queue: .main) { [weak self] in
            guard let self else { return }
            if stored == 0 {
                self.finish(failures > 0 ? "Couldn't add that." : "Nothing to add.")
                return
            }
            self.openApp()
        }
    }

    private func stage(_ attachment: NSItemProvider, done: @escaping (Result<Void, Error>) -> Void) {
        let name = attachment.suggestedName ?? "file"
        if attachment.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
            attachment.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { item, error in
                if let error {
                    done(.failure(error))
                    return
                }
                guard let url = item as? URL else {
                    done(.failure(InboxError.unavailable))
                    return
                }
                var isDirectory: ObjCBool = false
                if FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory), isDirectory.boolValue {
                    done(.failure(InboxError.unavailable))
                    return
                }
                do {
                    try DropInbox.stageFile(name: url.lastPathComponent.isEmpty ? name : url.lastPathComponent, mime: mimeFor(url.lastPathComponent), from: url)
                    done(.success(()))
                } catch {
                    done(.failure(error))
                }
            }
            return
        }
        if attachment.hasItemConformingToTypeIdentifier(UTType.image.identifier) {
            attachment.loadItem(forTypeIdentifier: UTType.image.identifier, options: nil) { item, error in
                if let error {
                    done(.failure(error))
                    return
                }
                do {
                    if let url = item as? URL {
                        try DropInbox.stageFile(name: url.lastPathComponent.isEmpty ? "\(name).png" : url.lastPathComponent, mime: mimeFor(url.lastPathComponent), from: url)
                    } else if let image = item as? UIImage, let png = image.pngData() {
                        try DropInbox.stage(kind: "file", name: name.contains(".") ? name : "\(name).png", mime: "image/png", body: png)
                    } else if let data = item as? Data {
                        try DropInbox.stage(kind: "file", name: name.contains(".") ? name : "\(name).img", mime: "application/octet-stream", body: data)
                    } else {
                        throw InboxError.unavailable
                    }
                    done(.success(()))
                } catch {
                    done(.failure(error))
                }
            }
            return
        }
        if attachment.hasItemConformingToTypeIdentifier(UTType.plainText.identifier) {
            attachment.loadItem(forTypeIdentifier: UTType.plainText.identifier, options: nil) { item, error in
                if let error {
                    done(.failure(error))
                    return
                }
                let text: String
                if let value = item as? String {
                    text = value
                } else if let value = item as? NSString {
                    text = value as String
                } else {
                    text = ""
                }
                if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    done(.failure(InboxError.unavailable))
                    return
                }
                do {
                    try DropInbox.stage(kind: "text", name: "", mime: "text/plain", body: Data(text.utf8))
                    done(.success(()))
                } catch {
                    done(.failure(error))
                }
            }
            return
        }
        if attachment.hasItemConformingToTypeIdentifier(UTType.url.identifier) {
            attachment.loadItem(forTypeIdentifier: UTType.url.identifier, options: nil) { item, error in
                if let error {
                    done(.failure(error))
                    return
                }
                let text = (item as? URL)?.absoluteString ?? ""
                guard !text.isEmpty else {
                    done(.failure(InboxError.unavailable))
                    return
                }
                do {
                    try DropInbox.stage(kind: "text", name: "", mime: "text/plain", body: Data(text.utf8))
                    done(.success(()))
                } catch {
                    done(.failure(error))
                }
            }
            return
        }
        attachment.loadFileRepresentation(forTypeIdentifier: UTType.data.identifier) { url, error in
            if let error {
                done(.failure(error))
                return
            }
            guard let url else {
                done(.failure(InboxError.unavailable))
                return
            }
            do {
                let filename = url.lastPathComponent.isEmpty ? name : url.lastPathComponent
                try DropInbox.stageFile(name: filename, mime: mimeFor(filename), from: url)
                done(.success(()))
            } catch {
                done(.failure(error))
            }
        }
    }

    private func openApp() {
        guard let url = URL(string: "dropclipboard://inbox") else {
            finish("Added. Open Drop to encrypt it.")
            return
        }
        extensionContext?.open(url, completionHandler: { [weak self] opened in
            DispatchQueue.main.async {
                if opened {
                    self?.extensionContext?.completeRequest(returningItems: nil)
                } else {
                    self?.finish("Added. Open Drop to encrypt it.")
                }
            }
        })
    }

    private func finish(_ message: String) {
        label.text = message
        DispatchQueue.main.asyncAfter(deadline: .now() + 1.2) { [weak self] in
            self?.extensionContext?.completeRequest(returningItems: nil)
        }
    }
}

private func mimeFor(_ name: String) -> String {
    switch (name as NSString).pathExtension.lowercased() {
    case "png": return "image/png"
    case "jpg", "jpeg": return "image/jpeg"
    case "gif": return "image/gif"
    case "webp": return "image/webp"
    case "svg": return "image/svg+xml"
    case "txt": return "text/plain"
    case "heic": return "image/heic"
    case "pdf": return "application/pdf"
    case "zip": return "application/zip"
    default: return "application/octet-stream"
    }
}
