import DropKit
import Foundation
import UIKit

/// Holds the signed-in client for the life of the process. Closing the app
/// drops the content key with the process. Nothing here writes it down.
final class SessionModel: ObservableObject {
    @Published var server: String
    @Published var username: String
    @Published var password = ""
    @Published var draft = ""
    @Published var account: DropAccount?
    @Published var items: [DropItem] = []
    @Published var error = ""
    @Published var status = ""
    @Published var busy = false
    @Published var http = false
    @Published var shareURL: URL?
    @Published var exportData: Data?
    @Published var exportName = "file"
    @Published var biometricsOn = false
    @Published var biometryAvailable = false
    @Published var biometryLabel = "Unlock with Face ID"
    @Published var biometryName = "Face ID"

    private var triedBiometrics = false
    private var biometricTicket = 0

    private var client: DropClient?
    private let work = DispatchQueue(label: "com.kiefermenard.drop.session")
    private var refreshTimer: Timer?

    init() {
        let settings = DropSettings.load()
        server = settings.serverURL
        username = settings.username
        http = server.hasPrefix("http://")
        let kind = BiometricStore.kind()
        biometryAvailable = kind != .none
        biometryLabel = kind.label
        biometryName = kind.name
        biometricsOn = BiometricStore.enrolled()
    }

    func signIn() {
        let server = self.server.trimmingCharacters(in: .whitespacesAndNewlines)
        let username = self.username.trimmingCharacters(in: .whitespacesAndNewlines)
        let password = self.password
        self.password = ""
        guard !server.isEmpty, !username.isEmpty, !password.isEmpty else {
            error = "Enter the server, username, and password."
            return
        }
        try? DropSettings(serverURL: server, username: username).save()
        busy = true
        error = ""
        status = "Signing in…"
        http = server.hasPrefix("http://")
        work.async { [weak self] in
            do {
                let next = try DropClient(server: server)
                let snapshot = try next.signIn(username: username, password: password)
                DispatchQueue.main.async {
                    guard let self else { return }
                    self.client = next
                    self.show(snapshot, status: "")
                    self.startRefresh()
                    self.drainInbox()
                    if BiometricStore.enrolled() {
                        self.refreshStoredUnlock(ticket: self.biometricTicket)
                    }
                }
            } catch {
                self?.fail(error)
            }
        }
    }

    func signOut() {
        biometricTicket += 1
        BiometricStore.delete()
        biometricsOn = false
        busy = true
        refreshTimer?.invalidate()
        refreshTimer = nil
        let current = client
        client = nil
        account = nil
        items = []
        work.async { [weak self] in
            current?.signOut()
            DispatchQueue.main.async {
                self?.busy = false
                self?.status = ""
            }
        }
    }

    func saveDraft() {
        let text = draft
        draft = ""
        guard let client else {
            error = DropError.locked.text
            return
        }
        busy = true
        error = ""
        work.async { [weak self] in
            do {
                let snapshot = try client.uploadText(text)
                DispatchQueue.main.async { self?.show(snapshot, status: "") }
            } catch {
                self?.fail(error)
            }
        }
    }

    func paste() {
        error = ""
        if let text = UIPasteboard.general.string, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            upload(text: text)
            return
        }
        if let image = UIPasteboard.general.image, let png = image.pngData() {
            upload(name: "pasted.png", mime: "image/png", data: png)
            return
        }
        error = "The clipboard is empty."
    }

    func uploadPicked(_ urls: [URL]) {
        guard client != nil else {
            error = DropError.locked.text
            return
        }
        busy = true
        error = ""
        work.async { [weak self] in
            var files: [(name: String, data: Data)] = []
            var failures: [String] = []
            for url in urls {
                let accessed = url.startAccessingSecurityScopedResource()
                defer {
                    if accessed { url.stopAccessingSecurityScopedResource() }
                }
                var isDirectory: ObjCBool = false
                if FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory), isDirectory.boolValue {
                    failures.append(DropError.directory.text)
                    continue
                }
                let name = url.lastPathComponent.isEmpty ? "file" : url.lastPathComponent
                guard let data = try? Data(contentsOf: url) else {
                    failures.append("Couldn't read \(name).")
                    continue
                }
                files.append((name: name, data: data))
            }
            self?.deliver(files, failures: failures)
        }
    }

    /// Camera photos and library photos use the same upload as a picked file.
    func uploadFiles(_ files: [(name: String, data: Data)], unread: Int = 0) {
        guard client != nil else {
            error = DropError.locked.text
            return
        }
        if files.isEmpty && unread == 0 { return }
        var failures: [String] = []
        if unread == 1 {
            failures.append("Couldn't read 1 photo.")
        } else if unread > 1 {
            failures.append("Couldn't read \(unread) photos.")
        }
        busy = true
        error = ""
        work.async { [weak self] in
            self?.deliver(files, failures: failures)
        }
    }

    func copy(_ item: DropItem) {
        guard let client else { return }
        busy = true
        error = ""
        work.async { [weak self] in
            do {
                let payload = try client.copyItem(item.id)
                DispatchQueue.main.async {
                    switch payload {
                    case .text(let text):
                        UIPasteboard.general.string = text
                        self?.status = "Copied."
                    case .image(let data):
                        if let image = UIImage(data: data) {
                            UIPasteboard.general.image = image
                            self?.status = "Copied."
                        } else {
                            self?.error = "Couldn't copy that image. Share it instead."
                        }
                    }
                    self?.busy = false
                }
            } catch {
                self?.fail(error)
            }
        }
    }

    func download(_ item: DropItem) {
        guard let client else { return }
        busy = true
        error = ""
        work.async { [weak self] in
            do {
                let file = try client.downloadItem(item.id)
                let url = FileManager.default.temporaryDirectory.appendingPathComponent(file.name)
                try file.bytes.write(to: url, options: .atomic)
                DispatchQueue.main.async {
                    self?.shareURL = url
                    self?.busy = false
                    self?.status = ""
                }
            } catch {
                self?.fail(error)
            }
        }
    }

    func saveFile(_ item: DropItem) {
        guard let client else { return }
        busy = true
        error = ""
        work.async { [weak self] in
            do {
                let file = try client.downloadItem(item.id)
                DispatchQueue.main.async {
                    self?.exportName = file.name.isEmpty ? "file" : file.name
                    self?.exportData = file.bytes
                    self?.busy = false
                    self?.status = ""
                }
            } catch {
                self?.fail(error)
            }
        }
    }

    func offerBiometrics() {
        guard !triedBiometrics, biometricsOn, account == nil else { return }
        triedBiometrics = true
        unlockWithBiometrics()
    }

    func unlockWithBiometrics() {
        guard biometricsOn, !busy else { return }
        let server = self.server.trimmingCharacters(in: .whitespacesAndNewlines)
        let username = self.username.trimmingCharacters(in: .whitespacesAndNewlines)
        busy = true
        error = ""
        status = "Unlocking…"
        http = server.hasPrefix("http://")
        // Face ID presents on the main thread. This call has to block somewhere else.
        work.async { [weak self] in
            switch BiometricStore.load() {
            case .canceled:
                DispatchQueue.main.async {
                    self?.busy = false
                    self?.status = ""
                }
            case .failed(let message):
                DispatchQueue.main.async {
                    self?.error = message
                    self?.busy = false
                    self?.status = ""
                }
            case .success(var data):
                defer { wipe(&data) }
                do {
                    let decoded = try UnlockBlob.decode(data)
                    var saved = decoded.server
                    while saved.hasSuffix("/") { saved.removeLast() }
                    var current = server
                    while current.hasSuffix("/") { current.removeLast() }
                    guard saved == current, decoded.username == username else {
                        throw DropError.message("Enter your password.")
                    }
                    let next = try DropClient(server: server)
                    let snapshot = try next.restore(data)
                    DispatchQueue.main.async {
                        guard let self else { return }
                        self.client = next
                        self.show(snapshot, status: "")
                        self.startRefresh()
                        self.drainInbox()
                    }
                } catch {
                    self?.fail(error)
                }
            }
        }
    }

    func setBiometrics(_ enabled: Bool) {
        biometricTicket += 1
        if !enabled {
            BiometricStore.delete()
            biometricsOn = false
            status = ""
            return
        }
        guard biometryAvailable else {
            error = "Biometrics aren't available on this phone."
            biometricsOn = false
            return
        }
        biometricsOn = true
        error = ""
        refreshStoredUnlock(ticket: biometricTicket)
    }

    func finishShare() {
        if let shareURL {
            try? FileManager.default.removeItem(at: shareURL)
        }
        shareURL = nil
    }

    func delete(_ item: DropItem) {
        guard let client else { return }
        busy = true
        work.async { [weak self] in
            do {
                let snapshot = try client.deleteItem(item.id)
                DispatchQueue.main.async { self?.show(snapshot, status: "") }
            } catch {
                self?.fail(error)
            }
        }
    }

    func drainInbox() {
        let staged = DropInbox.pending()
        guard !staged.isEmpty else { return }
        guard let client else {
            error = "Sign in to add the shared items."
            return
        }
        busy = true
        error = ""
        work.async { [weak self] in
            var last: DropSnapshot?
            var failures: [String] = []
            for item in staged {
                guard let data = DropInbox.readBody(item) else {
                    failures.append("Couldn't read a shared item.")
                    continue
                }
                do {
                    if item.kind == "text" {
                        last = try client.uploadText(String(decoding: data, as: UTF8.self))
                    } else {
                        let name = item.name.isEmpty ? "file" : item.name
                        last = try client.uploadFile(name: name, mime: item.mime, bytes: Array(data))
                    }
                    DropInbox.remove(item)
                } catch {
                    failures.append((error as? DropError)?.text ?? error.localizedDescription)
                }
            }
            DispatchQueue.main.async {
                if let last {
                    self?.show(last, status: "Added from the share sheet.")
                } else {
                    self?.busy = false
                }
                if !failures.isEmpty {
                    self?.error = failures.joined(separator: " ")
                }
            }
        }
    }

    private func deliver(_ files: [(name: String, data: Data)], failures: [String]) {
        guard let client else {
            DispatchQueue.main.async { [weak self] in
                self?.busy = false
                self?.error = DropError.locked.text
            }
            return
        }
        var failures = failures
        var last: DropSnapshot?
        var uploaded = 0
        for file in files {
            let name = file.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "file" : file.name
            do {
                last = try client.uploadFile(name: name, mime: mimeForFilename(name), bytes: Array(file.data))
                uploaded += 1
            } catch {
                failures.append((error as? DropError)?.text ?? error.localizedDescription)
            }
        }
        DispatchQueue.main.async { [weak self] in
            if let last {
                let message = uploaded == 1 ? "Uploaded 1 file." : "Uploaded \(uploaded) files."
                self?.show(last, status: failures.isEmpty ? message : "Uploaded \(uploaded).")
            } else {
                self?.busy = false
            }
            if !failures.isEmpty {
                self?.error = failures.joined(separator: " ")
                self?.status = ""
            }
        }
    }

    private func refreshStoredUnlock(ticket: Int) {
        guard let client else {
            biometricsOn = BiometricStore.enrolled()
            return
        }
        work.async { [weak self] in
            var bytes = Data()
            defer { wipe(&bytes) }
            let saved: BiometricSave
            do {
                bytes = try client.exportUnlock()
                saved = BiometricStore.save(bytes)
            } catch let error as DropError {
                saved = .failed(error.text)
            } catch {
                saved = .failed("Couldn't store biometric unlock.")
            }
            DispatchQueue.main.async {
                guard let self else { return }
                if self.biometricTicket != ticket || self.account == nil {
                    if case .success = saved {
                        BiometricStore.delete()
                    }
                    return
                }
                switch saved {
                case .success:
                    self.biometricsOn = true
                    self.status = ""
                case .canceled:
                    self.biometricsOn = BiometricStore.enrolled()
                case .failed(let message):
                    self.biometricsOn = BiometricStore.enrolled()
                    self.error = message
                }
            }
        }
    }

    private func upload(text: String) {
        guard let client else {
            error = DropError.locked.text
            return
        }
        busy = true
        error = ""
        work.async { [weak self] in
            do {
                let snapshot = try client.uploadText(text)
                DispatchQueue.main.async { self?.show(snapshot, status: "") }
            } catch {
                self?.fail(error)
            }
        }
    }

    private func upload(name: String, mime: String, data: Data) {
        guard let client else {
            error = DropError.locked.text
            return
        }
        busy = true
        error = ""
        work.async { [weak self] in
            do {
                let snapshot = try client.uploadFile(name: name, mime: mime, bytes: Array(data))
                DispatchQueue.main.async { self?.show(snapshot, status: "") }
            } catch {
                self?.fail(error)
            }
        }
    }

    private func show(_ snapshot: DropSnapshot, status: String) {
        account = snapshot.account
        items = snapshot.items
        http = snapshot.http
        busy = false
        error = ""
        self.status = status
    }

    private func fail(_ error: Error) {
        let signedOut = (error as? DropError) == .signedOut
        let text = (error as? DropError)?.text ?? error.localizedDescription
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            if signedOut {
                let current = self.client
                self.client = nil
                self.account = nil
                self.items = []
                self.refreshTimer?.invalidate()
                self.refreshTimer = nil
                self.work.async { current?.signOut() }
            }
            self.error = text
            self.status = ""
            self.busy = false
        }
    }

    private func startRefresh() {
        refreshTimer?.invalidate()
        refreshTimer = Timer.scheduledTimer(withTimeInterval: 4, repeats: true) { [weak self] _ in
            DispatchQueue.main.async {
                self?.refresh()
            }
        }
    }

    private func refresh() {
        guard let client, !busy else { return }
        work.async { [weak self] in
            do {
                let snapshot = try client.refresh()
                DispatchQueue.main.async {
                    guard let self, !self.busy else { return }
                    self.account = snapshot.account
                    self.items = snapshot.items
                    self.http = snapshot.http
                }
            } catch DropError.signedOut {
                self?.fail(DropError.signedOut)
            } catch {
                return
            }
        }
    }
}
