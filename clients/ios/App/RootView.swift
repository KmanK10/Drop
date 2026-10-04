import DropKit
import SwiftUI
import UniformTypeIdentifiers

struct RootView: View {
    @EnvironmentObject private var model: SessionModel
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        NavigationStack {
            Group {
                if model.account == nil {
                    SignInView()
                } else {
                    ClipboardView()
                }
            }
            .padding(20)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .background(DropColor.cream.ignoresSafeArea())
        }
        .onAppear { model.drainInbox() }
        .onChange(of: scenePhase) { _, phase in
            if phase == .active { model.drainInbox() }
        }
    }
}

private struct SignInView: View {
    @EnvironmentObject private var model: SessionModel

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                Wordmark()
                Text("Sign in")
                    .font(.title2.weight(.semibold))
                    .foregroundStyle(DropColor.ink)
                Text("The password unlocks items on this phone. It stays in memory until you close Drop, and it is not saved.")
                    .font(.subheadline)
                    .foregroundStyle(DropColor.muted)
                field("Server", text: $model.server, secure: false, placeholder: "https://drop.example")
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .keyboardType(.URL)
                    .textContentType(.none)
                field("Username", text: $model.username, secure: false)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .textContentType(.none)
                field("Password", text: $model.password, secure: true)
                    .textContentType(.none)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                if model.http || model.server.hasPrefix("http://") {
                    Text("This connection is not HTTPS. The password still stays on this phone, but the network can see the session.")
                        .font(.footnote)
                        .foregroundStyle(DropColor.muted)
                }
                Notice()
                Button(model.busy ? "Signing in…" : "Sign in") {
                    model.signIn()
                }
                .buttonStyle(PrimaryButtonStyle())
                .disabled(model.busy)
                Text("Accounts are invite-only. Ask the person who runs this Drop for a username. There is no public signup.")
                    .font(.footnote)
                    .foregroundStyle(DropColor.muted)
            }
        }
    }

    private func field(_ title: String, text: Binding<String>, secure: Bool, placeholder: String? = nil) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).foregroundStyle(DropColor.ink)
            Group {
                if secure {
                    SecureField(placeholder ?? title, text: text)
                } else {
                    TextField(placeholder ?? title, text: text)
                }
            }
            .padding(12)
            .background(DropColor.card)
            .clipShape(RoundedRectangle(cornerRadius: 10))
            .overlay(RoundedRectangle(cornerRadius: 10).stroke(DropColor.line))
        }
    }
}

private struct ClipboardView: View {
    @EnvironmentObject private var model: SessionModel

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Wordmark()
                Spacer()
                Button("Sign out") { model.signOut() }
                    .disabled(model.busy)
                    .foregroundStyle(DropColor.ink)
            }
            if let account = model.account {
                Text("\(account.username) · \(DropFormat.bytes(account.usedBytes)) of \(DropFormat.bytes(account.quotaBytes)) · kept \(DropFormat.retention(account.ttlMs))")
                    .font(.footnote)
                    .foregroundStyle(DropColor.muted)
            }
            if model.http {
                Text("This connection is not HTTPS. Items are still encrypted before they are uploaded.")
                    .font(.footnote)
                    .foregroundStyle(DropColor.muted)
            }
            TextEditor(text: $model.draft)
                .frame(minHeight: 96)
                .padding(8)
                .scrollContentBackground(.hidden)
                .background(DropColor.card)
                .clipShape(RoundedRectangle(cornerRadius: 10))
                .overlay(RoundedRectangle(cornerRadius: 10).stroke(DropColor.line))
            HStack {
                Button("Save text") { model.saveDraft() }
                    .buttonStyle(PrimaryButtonStyle())
                    .disabled(model.busy)
                Button("Paste") { model.paste() }
                    .disabled(model.busy)
                Button("Upload file…") { model.pickingFile = true }
                    .disabled(model.busy)
            }
            .foregroundStyle(DropColor.ink)
            Notice()
            ScrollView {
                if model.items.isEmpty {
                    Text("Nothing here yet. Share a file into Drop, or save a note.")
                        .font(.subheadline)
                        .foregroundStyle(DropColor.muted)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                ForEach(model.items) { item in
                    ItemCard(item: item)
                }
            }
            Text("Drop stays unlocked until you close the app. Closing it forgets the key.")
                .font(.caption)
                .foregroundStyle(DropColor.muted)
        }
        .fileImporter(isPresented: $model.pickingFile, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            if case .success(let urls) = result {
                model.uploadPicked(urls)
            }
        }
        .sheet(isPresented: Binding(
            get: { model.shareURL != nil },
            set: { if !$0 { model.finishShare() } }
        )) {
            if let url = model.shareURL {
                ActivityView(url: url) { model.finishShare() }
            }
        }
    }
}

private struct ItemCard: View {
    @EnvironmentObject private var model: SessionModel
    let item: DropItem

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(item.title).font(.body.weight(.semibold)).foregroundStyle(DropColor.ink)
            Text(item.detail).font(.footnote).foregroundStyle(DropColor.muted)
            HStack {
                if item.canCopy {
                    Button("Copy") { model.copy(item) }.disabled(model.busy)
                }
                Button("Share") { model.download(item) }.disabled(model.busy)
                Button(model.pendingDelete == item.id ? "Delete now" : "Delete") {
                    model.delete(item)
                }
                .disabled(model.busy)
            }
            .font(.subheadline)
            .foregroundStyle(DropColor.green)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(DropColor.card)
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .overlay(RoundedRectangle(cornerRadius: 12).stroke(DropColor.line))
    }
}

private struct Notice: View {
    @EnvironmentObject private var model: SessionModel

    var body: some View {
        if !model.error.isEmpty {
            Text(model.error).font(.subheadline).foregroundStyle(DropColor.danger)
        } else if !model.status.isEmpty {
            Text(model.status).font(.subheadline).foregroundStyle(DropColor.muted)
        }
    }
}

private struct Wordmark: View {
    var body: some View {
        HStack(spacing: 10) {
            ZStack {
                Circle().fill(DropColor.green).frame(width: 36, height: 36)
                Text("D").font(.headline).foregroundStyle(DropColor.onGreen)
            }
            VStack(alignment: .leading, spacing: 0) {
                Text("Drop").font(.title3.weight(.semibold)).foregroundStyle(DropColor.ink)
                Text("Private clipboard").font(.caption).foregroundStyle(DropColor.muted)
            }
        }
    }
}

private struct PrimaryButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.body.weight(.semibold))
            .foregroundStyle(DropColor.onGreen)
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .background(DropColor.green.opacity(configuration.isPressed ? 0.85 : 1))
            .clipShape(RoundedRectangle(cornerRadius: 10))
    }
}

private struct ActivityView: UIViewControllerRepresentable {
    let url: URL
    var onDone: () -> Void

    func makeUIViewController(context: Context) -> UIActivityViewController {
        let controller = UIActivityViewController(activityItems: [url], applicationActivities: nil)
        controller.completionWithItemsHandler = { _, _, _, _ in onDone() }
        return controller
    }

    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}

enum DropColor {
    static let green = Color(red: 0x1d / 255, green: 0x68 / 255, blue: 0x43 / 255)
    static let cream = Color(red: 0xf7 / 255, green: 0xf3 / 255, blue: 0xea / 255)
    static let card = Color(red: 0xff / 255, green: 0xfd / 255, blue: 0xf8 / 255)
    static let ink = Color(red: 0x1c / 255, green: 0x19 / 255, blue: 0x15 / 255)
    static let muted = Color(red: 0x6d / 255, green: 0x66 / 255, blue: 0x5c / 255)
    static let danger = Color(red: 0x9d / 255, green: 0x34 / 255, blue: 0x1c / 255)
    static let line = Color(red: 0xe4 / 255, green: 0xda / 255, blue: 0xc9 / 255)
    static let onGreen = Color(red: 0xf4 / 255, green: 0xff / 255, blue: 0xf7 / 255)
}
