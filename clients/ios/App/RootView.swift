import DropKit
import PhotosUI
import SwiftUI
import UIKit
import UniformTypeIdentifiers

struct RootView: View {
    @EnvironmentObject private var model: SessionModel
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.colorScheme) private var colorScheme
    /// Settings is pushed from the stack root, which stays in place across
    /// sign-in and sign-out, so clearing the session can pop it.
    @State private var showSettings = false

    private var palette: DropPalette { DropPalette.forScheme(colorScheme) }

    var body: some View {
        NavigationStack {
            Group {
                if model.account == nil {
                    SignInView()
                } else {
                    ClipboardView(showSettings: $showSettings)
                }
            }
            .padding(20)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .background(palette.background.ignoresSafeArea())
            .navigationDestination(isPresented: $showSettings) {
                SettingsView()
            }
        }
        .onChange(of: model.account == nil) { _, signedOut in
            // Sign out, Delete account, and an expired session all clear the
            // account. Return to the sign-in screen instead of leaving Settings up.
            if signedOut { showSettings = false }
        }
        .background(palette.background.ignoresSafeArea())
        .toolbarBackground(palette.background, for: .navigationBar)
        .toolbarColorScheme(colorScheme == .dark ? .dark : .light, for: .navigationBar)
        .background(KeyboardDismissAnchor())
        .onAppear {
            model.drainInbox()
            KeyboardDismiss.installOnKeyWindow()
        }
        .onChange(of: scenePhase) { _, phase in
            if phase == .active { model.drainInbox() }
        }
    }
}

private struct SignInView: View {
    @EnvironmentObject private var model: SessionModel
    @Environment(\.colorScheme) private var colorScheme
    @State private var showSetup = false
    @State private var setupServer = ""
    @State private var setupUsername = ""
    @State private var returning = false
    @State private var decided = false
    @State private var usePassword = false

    private var palette: DropPalette { DropPalette.forScheme(colorScheme) }
    private var setup: Bool { showSetup || !returning }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                Wordmark()
                Text("Sign in")
                    .font(.title2.weight(.semibold))
                    .foregroundStyle(palette.ink)
                if setup {
                    if !model.biometricsOn {
                        Text("The password unlocks items on this phone. It stays in memory until you close Drop, and it is not saved.")
                            .font(.subheadline)
                            .foregroundStyle(palette.muted)
                    }
                    if showSetup && returning {
                        field("Server", text: $setupServer, secure: false, placeholder: "https://drop.example")
                        field("Username", text: $setupUsername, secure: false)
                    } else {
                        field("Server", text: $model.server, secure: false, placeholder: "https://drop.example")
                        field("Username", text: $model.username, secure: false)
                    }
                    field("Password", text: $model.password, secure: true)
                    httpNote
                    if !model.error.isEmpty {
                        Text(model.error).font(.subheadline).foregroundStyle(palette.danger)
                    }
                    Button(model.status == "Signing in…" ? "Signing in…" : "Sign in") {
                        signInFromSetup()
                    }
                    .buttonStyle(PrimaryButtonStyle())
                    .disabled(model.busy)
                    if returning {
                        Button("Back") {
                            showSetup = false
                            model.password = ""
                            model.error = ""
                        }
                        .disabled(model.busy)
                        .foregroundStyle(palette.ink)
                    }
                    Text("Accounts are invite-only. Ask the person who runs this Drop for a username. There is no public signup.")
                        .font(.footnote)
                        .foregroundStyle(palette.muted)
                } else if model.pinOn && !usePassword {
                    field("PIN", text: $model.pinEntry, secure: true, keyboard: .numberPad)
                    httpNote
                    if !model.error.isEmpty {
                        Text(model.error).font(.subheadline).foregroundStyle(palette.danger)
                    }
                    if model.biometricsOn {
                        HStack {
                            Spacer(minLength: 8)
                            biometricMark
                        }
                    }
                    usePasswordButton
                    setupButton
                } else {
                    field("Password", text: $model.password, secure: true)
                    httpNote
                    if !model.error.isEmpty {
                        Text(model.error).font(.subheadline).foregroundStyle(palette.danger)
                    }
                    signInRow
                    if model.pinOn {
                        usePinButton
                    }
                    setupButton
                }
            }
        }
        .scrollDismissesKeyboard(.interactively)
        .onAppear {
            if !decided {
                let server = model.server.trimmingCharacters(in: .whitespacesAndNewlines)
                let username = model.username.trimmingCharacters(in: .whitespacesAndNewlines)
                returning = !server.isEmpty && !username.isEmpty
                decided = true
            }
            model.offerBiometrics()
        }
        .onChange(of: model.pinEntry) { _, _ in
            if returning && !showSetup && model.pinOn && !usePassword {
                model.notePinEntry()
            }
        }
    }

    private var httpNote: some View {
        Group {
            if model.http || model.server.hasPrefix("http://") {
                Text("This connection is not HTTPS. The password still stays on this phone, but the network can see the session.")
                    .font(.footnote)
                    .foregroundStyle(palette.muted)
            }
        }
    }

    private var signInRow: some View {
        HStack(alignment: .center, spacing: 12) {
            Button(model.status == "Signing in…" ? "Signing in…" : "Sign in") {
                if model.password.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    model.error = "Enter your password."
                } else {
                    model.signIn()
                }
            }
            .buttonStyle(PrimaryButtonStyle())
            .disabled(model.busy)
            Spacer(minLength: 8)
            biometricMark
        }
    }

    @ViewBuilder
    private var biometricMark: some View {
        if model.biometricsOn {
            Button {
                model.unlockWithBiometrics()
            } label: {
                Image(systemName: model.biometrySymbol)
                    .font(.system(size: 28))
                    .foregroundStyle(palette.ink)
                    .frame(width: 44, height: 44)
                    .contentShape(Rectangle())
                    .accessibilityLabel(model.biometryLabel)
            }
            .buttonStyle(.plain)
            .disabled(model.busy)
        }
    }

    private var usePasswordButton: some View {
        Button("Use password") {
            model.pinEntry = ""
            model.password = ""
            model.error = ""
            usePassword = true
        }
        .disabled(model.busy)
        .foregroundStyle(palette.ink)
    }

    private var usePinButton: some View {
        Button("Use PIN") {
            model.password = ""
            model.pinEntry = ""
            model.error = ""
            usePassword = false
        }
        .disabled(model.busy)
        .foregroundStyle(palette.ink)
    }

    private var setupButton: some View {
        Button("Change server or account") {
            setupServer = model.server
            setupUsername = model.username
            model.password = ""
            model.pinEntry = ""
            model.error = ""
            showSetup = true
        }
        .disabled(model.busy)
        .foregroundStyle(palette.ink)
    }

    private func signInFromSetup() {
        guard showSetup && returning else {
            model.signIn()
            return
        }
        let server = setupServer.trimmingCharacters(in: .whitespacesAndNewlines)
        let username = setupUsername.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !server.isEmpty, !username.isEmpty, !model.password.isEmpty else {
            model.error = "Enter the server, username, and password."
            return
        }
        model.server = server
        model.username = username
        model.signIn()
    }

    private func field(_ title: String, text: Binding<String>, secure: Bool, placeholder: String? = nil, keyboard: UIKeyboardType? = nil) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).foregroundStyle(palette.ink)
            LineField(
                text: text,
                placeholder: placeholder ?? "",
                secure: secure,
                ink: UIColor(palette.ink),
                muted: UIColor(palette.muted),
                caret: UIColor(palette.green),
                keyboard: keyboard ?? (title == "Server" ? .URL : .default)
            )
            .padding(12)
            .background(palette.card)
            .clipShape(RoundedRectangle(cornerRadius: 10))
            .overlay(RoundedRectangle(cornerRadius: 10).stroke(palette.line))
        }
    }
}

private enum AddChoice {
    case camera
    case photo
    case file
}

private struct ClipboardView: View {
    @EnvironmentObject private var model: SessionModel
    @Environment(\.colorScheme) private var colorScheme
    @State private var takingPhoto = false
    @State private var choosingPhoto = false
    @State private var pickingFile = false
    @Binding var showSettings: Bool

    private var palette: DropPalette { DropPalette.forScheme(colorScheme) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Wordmark()
                Spacer()
                Button {
                    showSettings = true
                } label: {
                    Image(systemName: "gearshape")
                        .font(.system(size: 22, weight: .semibold))
                        .foregroundStyle(palette.ink)
                        .frame(width: 44, height: 44)
                        .contentShape(Rectangle())
                        .accessibilityLabel("Settings")
                }
                .buttonStyle(.plain)
            }
            if let account = model.account {
                Text("\(account.username) · \(DropFormat.bytes(account.usedBytes)) of \(DropFormat.bytes(account.quotaBytes)) · kept \(DropFormat.retention(account.ttlMs))")
                    .font(.footnote)
                    .foregroundStyle(palette.muted)
            }
            if model.http {
                Text("This connection is not HTTPS. Items are still encrypted before they are uploaded.")
                    .font(.footnote)
                    .foregroundStyle(palette.muted)
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 12) {
                    ClipboardEditor(text: $model.draft, ink: UIColor(palette.ink), caret: UIColor(palette.green))
                        .frame(minHeight: 96)
                        .padding(8)
                        .background(palette.card)
                        .clipShape(RoundedRectangle(cornerRadius: 10))
                        .overlay(RoundedRectangle(cornerRadius: 10).stroke(palette.line))
                    HStack {
                        Button("Save text") { model.saveDraft() }
                            .buttonStyle(PrimaryButtonStyle())
                            .disabled(model.busy)
                        Button("Paste") { model.paste() }
                            .disabled(model.busy)
                        Menu {
                            Button("Take a photo") { presentAdd(.camera) }
                            Button("Choose a photo") { presentAdd(.photo) }
                            Button("Choose a file") { presentAdd(.file) }
                        } label: {
                            CircleMark(systemName: "plus", ink: palette.ink, label: "Add")
                        }
                        .menuOrder(.fixed)
                        .buttonStyle(.plain)
                        .fixedSize()
                        .disabled(model.busy)
                    }
                    .foregroundStyle(palette.ink)
                    Notice()
                    if model.items.isEmpty {
                        Text("Nothing here yet. Share a file into Drop, or save a note.")
                            .font(.subheadline)
                            .foregroundStyle(palette.muted)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    ForEach(model.items) { item in
                        ItemCard(item: item)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .scrollDismissesKeyboard(.interactively)
        }
        .toolbar(.hidden, for: .navigationBar)
        .fullScreenCover(isPresented: $takingPhoto) {
            CameraPicker(
                isPresented: $takingPhoto,
                onCapture: { name, data in
                    model.uploadFiles([(name: name, data: data)])
                },
                onFailure: {
                    model.error = "Couldn't read that photo."
                }
            )
        }
        .sheet(isPresented: $choosingPhoto) {
            PhotoLibraryPicker(isPresented: $choosingPhoto) { files, unread in
                if files.isEmpty && unread == 0 {
                    model.error = "Couldn't read that photo."
                } else {
                    model.uploadFiles(files, unread: unread)
                }
            }
        }
        .fileImporter(isPresented: $pickingFile, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            if case .success(let urls) = result {
                model.uploadPicked(urls)
            }
        }
        .fileExporter(
            isPresented: Binding(
                get: { model.exportData != nil },
                set: { if !$0 { model.exportData = nil } }
            ),
            document: model.exportData.map(SavedFile.init(data:)),
            contentType: .data,
            defaultFilename: model.exportName
        ) { result in
            if case .failure = result {
                model.error = "Couldn't save that file."
            }
            model.exportData = nil
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

    /// The menu has to finish dismissing before the camera, photo library, or
    /// file picker can be presented.
    private func presentAdd(_ choice: AddChoice) {
        DispatchQueue.main.async {
            switch choice {
            case .camera:
                if UIImagePickerController.isSourceTypeAvailable(.camera) {
                    takingPhoto = true
                } else {
                    model.error = "This device has no camera."
                }
            case .photo:
                choosingPhoto = true
            case .file:
                pickingFile = true
            }
        }
    }
}

/// A single-line field. SwiftUI's text color follows the system, so a light
/// card in dark mode draws white text. These colors are set on the UITextField.
private struct LineField: UIViewRepresentable {
    @Binding var text: String
    var placeholder: String
    var secure: Bool
    var ink: UIColor
    var muted: UIColor
    var caret: UIColor
    var keyboard: UIKeyboardType
    var enabled = true

    func makeCoordinator() -> Coordinator {
        Coordinator(text: $text)
    }

    func makeUIView(context: Context) -> UITextField {
        let field = UITextField()
        field.delegate = context.coordinator
        field.addTarget(context.coordinator, action: #selector(Coordinator.changed(_:)), for: .editingChanged)
        field.borderStyle = .none
        field.backgroundColor = .clear
        field.isSecureTextEntry = secure
        field.keyboardType = keyboard
        field.autocapitalizationType = .none
        field.autocorrectionType = .no
        field.textContentType = .none
        field.isEnabled = enabled
        field.font = .preferredFont(forTextStyle: .body)
        field.adjustsFontForContentSizeCategory = true
        field.setContentHuggingPriority(.defaultLow, for: .horizontal)
        field.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        field.setContentHuggingPriority(.required, for: .vertical)
        field.setContentCompressionResistancePriority(.required, for: .vertical)
        apply(field)
        field.text = text
        return field
    }

    func updateUIView(_ field: UITextField, context: Context) {
        context.coordinator.text = $text
        apply(field)
        if field.text != text {
            field.text = text
        }
    }

    private func apply(_ field: UITextField) {
        field.textColor = ink
        field.tintColor = caret
        field.isEnabled = enabled
        field.attributedPlaceholder = NSAttributedString(
            string: placeholder,
            attributes: [.foregroundColor: muted]
        )
    }

    final class Coordinator: NSObject, UITextFieldDelegate {
        var text: Binding<String>

        init(text: Binding<String>) {
            self.text = text
        }

        @objc func changed(_ field: UITextField) {
            text.wrappedValue = field.text ?? ""
        }
    }
}

/// The clipboard note. A SwiftUI TextEditor keeps the system text color, which
/// is white in dark mode even when the field behind it is light.
private struct ClipboardEditor: UIViewRepresentable {
    @Binding var text: String
    var ink: UIColor
    var caret: UIColor

    func makeCoordinator() -> Coordinator {
        Coordinator(text: $text)
    }

    func makeUIView(context: Context) -> UITextView {
        let view = UITextView()
        view.delegate = context.coordinator
        view.backgroundColor = .clear
        view.textColor = ink
        view.tintColor = caret
        view.font = .preferredFont(forTextStyle: .body)
        view.adjustsFontForContentSizeCategory = true
        view.text = text
        view.textContainerInset = UIEdgeInsets(top: 8, left: 4, bottom: 8, right: 4)
        return view
    }

    func updateUIView(_ view: UITextView, context: Context) {
        context.coordinator.text = $text
        if view.text != text {
            view.text = text
        }
        view.textColor = ink
        view.tintColor = caret
    }

    final class Coordinator: NSObject, UITextViewDelegate {
        var text: Binding<String>

        init(text: Binding<String>) {
            self.text = text
        }

        func textViewDidChange(_ textView: UITextView) {
            text.wrappedValue = textView.text
        }
    }
}

private struct ItemCard: View {
    @EnvironmentObject private var model: SessionModel
    @Environment(\.colorScheme) private var colorScheme
    let item: DropItem

    private var palette: DropPalette { DropPalette.forScheme(colorScheme) }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(item.title)
                .font(.body.weight(.semibold))
                .foregroundStyle(palette.ink)
                .lineLimit(item.kind == "text" ? 4 : nil)
            TimelineView(.everyMinute) { _ in
                let expiresIn = DropFormat.daysLeft(
                    createdAt: item.createdAt,
                    ttlMs: model.account?.ttlMs ?? 0,
                    nowMs: DropFormat.nowMs()
                )
                HStack(spacing: 0) {
                    Text("\(item.detail) · \(item.when)")
                        .font(.footnote)
                        .foregroundStyle(palette.muted)
                    if let expiresIn {
                        Text(" · \(expiresIn)")
                            .font(.footnote)
                            .foregroundStyle(palette.danger)
                    }
                }
            }
            if let preview = item.previewText, !preview.isEmpty {
                Text(preview)
                    .font(.subheadline)
                    .foregroundStyle(palette.ink)
                    .lineLimit(4)
            }
            if let preview = item.previewImage, let image = UIImage(data: preview) {
                Image(uiImage: image)
                    .resizable()
                    .scaledToFit()
                    .frame(maxWidth: 160, maxHeight: 120, alignment: .leading)
                    .clipShape(RoundedRectangle(cornerRadius: 8))
                    .accessibilityLabel(item.title)
            }
            HStack(spacing: 8) {
                if item.canCopy {
                    Button("Copy") { model.copy(item) }
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(palette.green)
                        .buttonStyle(.plain)
                        .disabled(model.busy)
                }
                Menu {
                    Button("Share") { model.download(item) }
                    Button("Download") { model.saveFile(item) }
                } label: {
                    CircleMark(systemName: "plus", ink: palette.green, label: "Shortcuts")
                }
                .menuOrder(.fixed)
                .buttonStyle(.plain)
                .fixedSize()
                .disabled(model.busy)
                Spacer(minLength: 8)
                Button {
                    model.delete(item)
                } label: {
                    CircleMark(systemName: "trash", ink: palette.danger, label: "Delete")
                }
                .buttonStyle(.plain)
                .disabled(model.busy)
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(palette.card)
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .overlay(RoundedRectangle(cornerRadius: 12).stroke(palette.line))
    }
}

private enum SettingsForm {
    case none
    case setPin
    case changePin
    case password
}

private struct SettingsView: View {
    @EnvironmentObject private var model: SessionModel
    @Environment(\.colorScheme) private var colorScheme
    @State private var form = SettingsForm.none
    @State private var current = ""
    @State private var next = ""
    @State private var confirm = ""
    @State private var pin = ""
    @State private var pinConfirm = ""
    @State private var currentPin = ""
    @State private var confirmingDelete = false
    @State private var deleteName = ""

    private var palette: DropPalette { DropPalette.forScheme(colorScheme) }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                if model.biometryAvailable {
                    Toggle(model.biometryLabel, isOn: Binding(
                        get: { model.biometricsOn },
                        set: { model.setBiometrics($0) }
                    ))
                    .disabled(model.busy)
                    .foregroundStyle(palette.ink)
                }
                if model.pinOn {
                    Toggle("PIN", isOn: Binding(
                        get: { model.pinOn },
                        set: { model.setPin($0) }
                    ))
                    .disabled(model.busy)
                    .foregroundStyle(palette.ink)
                }
                if form == .none {
                    Group {
                        if model.pinOn {
                            Button("Change PIN") { form = .changePin }
                        } else {
                            Button("Set PIN") { form = .setPin }
                        }
                        Button("Change password") { form = .password }
                    }
                    .disabled(model.busy)
                    .foregroundStyle(palette.ink)
                }
                if form == .setPin || form == .changePin {
                    if form == .changePin {
                        field("Current PIN", text: $currentPin, keyboard: .numberPad)
                    }
                    field("New PIN", text: $pin, keyboard: .numberPad)
                    field("Confirm PIN", text: $pinConfirm, keyboard: .numberPad)
                    Text("Use 4 to 8 digits.")
                        .font(.footnote)
                        .foregroundStyle(palette.muted)
                    Button(model.settingsBusy.isEmpty ? "Save" : model.settingsBusy) {
                        model.savePin(current: currentPin, newPin: pin, confirm: pinConfirm, changing: form == .changePin)
                    }
                    .buttonStyle(PrimaryButtonStyle())
                    .disabled(model.busy)
                    Button("Cancel") { clearForm() }
                        .disabled(model.busy)
                }
                if form == .password {
                    field("Current password", text: $current)
                    field("New password", text: $next)
                    field("Confirm new password", text: $confirm)
                    if model.pinOn {
                        Text("Changing the password turns the PIN off.")
                            .font(.footnote)
                            .foregroundStyle(palette.muted)
                    }
                    Button(model.settingsBusy.isEmpty ? "Save" : model.settingsBusy) {
                        model.changePassword(current: current, next: next, confirm: confirm)
                    }
                    .buttonStyle(PrimaryButtonStyle())
                    .disabled(model.busy)
                    Button("Cancel") { clearForm() }
                        .disabled(model.busy)
                }
                if !model.settingsError.isEmpty {
                    Text(model.settingsError).font(.subheadline).foregroundStyle(palette.danger)
                }
                if !model.settingsStatus.isEmpty {
                    Text(model.settingsStatus).font(.subheadline).foregroundStyle(palette.muted)
                }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .scrollDismissesKeyboard(.interactively)
            Button("Sign out") { model.signOut() }
                .disabled(model.busy)
                .foregroundStyle(palette.ink)
            VStack(alignment: .leading, spacing: 10) {
                Text("Danger zone")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(palette.danger)
                if confirmingDelete {
                    field("Username", text: $deleteName, secure: false)
                        .fixedSize(horizontal: false, vertical: true)
                    if !model.settingsError.isEmpty {
                        Text(model.settingsError).font(.subheadline).foregroundStyle(palette.danger)
                    }
                    Button(model.settingsBusy.isEmpty ? "Delete account" : model.settingsBusy) {
                        model.deleteAccount(typed: deleteName)
                    }
                    .disabled(model.busy)
                    .foregroundStyle(palette.danger)
                    Button("Cancel") {
                        confirmingDelete = false
                        deleteName = ""
                        model.settingsError = ""
                    }
                    .disabled(model.busy)
                    .foregroundStyle(palette.ink)
                } else {
                    Button("Delete account") {
                        confirmingDelete = true
                        deleteName = ""
                        model.settingsError = ""
                    }
                    .disabled(model.busy)
                    .foregroundStyle(palette.danger)
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(12)
            .background(palette.card)
            .clipShape(RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12).stroke(palette.danger.opacity(0.45)))
        }
        .padding(20)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(palette.background.ignoresSafeArea())
        .navigationTitle("Settings")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar(.visible, for: .navigationBar)
        .toolbarBackground(palette.background, for: .navigationBar)
        .toolbarBackground(.visible, for: .navigationBar)
        .toolbarColorScheme(colorScheme == .dark ? .dark : .light, for: .navigationBar)
        .onChange(of: model.formDone) { _, _ in
            clearForm()
        }
    }

    private func clearForm() {
        form = .none
        current = ""
        next = ""
        confirm = ""
        pin = ""
        pinConfirm = ""
        currentPin = ""
    }

    private func field(_ title: String, text: Binding<String>, secure: Bool = true, keyboard: UIKeyboardType = .default) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(title).foregroundStyle(palette.ink)
            LineField(
                text: text,
                placeholder: "",
                secure: secure,
                ink: UIColor(palette.ink),
                muted: UIColor(palette.muted),
                caret: UIColor(palette.green),
                keyboard: keyboard,
                enabled: !model.busy
            )
            .padding(12)
            .background(palette.card)
            .clipShape(RoundedRectangle(cornerRadius: 10))
            .overlay(RoundedRectangle(cornerRadius: 10).stroke(palette.line))
        }
    }
}

private struct Notice: View {
    @EnvironmentObject private var model: SessionModel
    @Environment(\.colorScheme) private var colorScheme

    private var palette: DropPalette { DropPalette.forScheme(colorScheme) }

    var body: some View {
        if !model.error.isEmpty {
            Text(model.error).font(.subheadline).foregroundStyle(palette.danger)
        } else if !model.status.isEmpty {
            Text(model.status).font(.subheadline).foregroundStyle(palette.muted)
        }
    }
}

private struct Wordmark: View {
    @Environment(\.colorScheme) private var colorScheme

    private var palette: DropPalette { DropPalette.forScheme(colorScheme) }

    var body: some View {
        HStack(spacing: 10) {
            ZStack {
                Circle().fill(palette.green).frame(width: 36, height: 36)
                Text("D").font(.headline).foregroundStyle(palette.onGreen)
            }
            VStack(alignment: .leading, spacing: 0) {
                Text("Drop").font(.title3.weight(.semibold)).foregroundStyle(palette.ink)
                Text("Private clipboard").font(.caption).foregroundStyle(palette.muted)
            }
        }
    }
}

private struct PrimaryButtonStyle: ButtonStyle {
    @Environment(\.colorScheme) private var colorScheme

    func makeBody(configuration: Configuration) -> some View {
        let palette = DropPalette.forScheme(colorScheme)
        configuration.label
            .font(.body.weight(.semibold))
            .foregroundStyle(palette.onGreen)
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .background(palette.green.opacity(configuration.isPressed ? 0.85 : 1))
            .clipShape(RoundedRectangle(cornerRadius: 10))
    }
}

private struct CircleMark: View {
    var systemName: String
    var ink: Color
    var label: String
    var filled = false
    var mark: Color?

    var body: some View {
        Image(systemName: systemName)
            .font(.body.weight(.semibold))
            .foregroundStyle(filled ? (mark ?? ink) : ink)
            .frame(width: 36, height: 36)
            .background {
                if filled {
                    Circle().fill(ink)
                }
            }
            .overlay(Circle().stroke(ink, lineWidth: 1.5))
            .contentShape(Circle())
            .accessibilityLabel(label)
    }
}

private struct SavedFile: FileDocument {
    static var readableContentTypes: [UTType] { [.data] }
    var data: Data

    init(data: Data) {
        self.data = data
    }

    init(configuration: ReadConfiguration) throws {
        data = configuration.file.regularFileContents ?? Data()
    }

    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper {
        FileWrapper(regularFileWithContents: data)
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

private struct CameraPicker: UIViewControllerRepresentable {
    @Binding var isPresented: Bool
    var onCapture: (String, Data) -> Void
    var onFailure: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    func makeUIViewController(context: Context) -> UIImagePickerController {
        let picker = UIImagePickerController()
        picker.sourceType = .camera
        picker.mediaTypes = [UTType.image.identifier]
        picker.cameraCaptureMode = .photo
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ picker: UIImagePickerController, context: Context) {
        context.coordinator.parent = self
    }

    final class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
        var parent: CameraPicker

        init(parent: CameraPicker) {
            self.parent = parent
        }

        func imagePickerControllerDidCancel(_ picker: UIImagePickerController) {
            parent.isPresented = false
        }

        func imagePickerController(
            _ picker: UIImagePickerController,
            didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]
        ) {
            parent.isPresented = false
            guard let image = info[.originalImage] as? UIImage, let data = image.jpegData(compressionQuality: 0.92) else {
                parent.onFailure()
                return
            }
            parent.onCapture(Self.photoName(), data)
        }

        private static func photoName() -> String {
            let formatter = DateFormatter()
            formatter.locale = Locale(identifier: "en_US_POSIX")
            formatter.dateFormat = "yyyyMMdd-HHmmss"
            return "photo-\(formatter.string(from: Date())).jpg"
        }
    }
}

private final class PhotoSlots {
    private var values: [(name: String, data: Data)?]
    private let lock = NSLock()

    init(count: Int) {
        values = Array(repeating: nil, count: count)
    }

    func put(_ index: Int, name: String, data: Data) {
        lock.lock()
        values[index] = (name: name, data: data)
        lock.unlock()
    }

    func files() -> [(name: String, data: Data)] {
        lock.lock()
        let ready = values.compactMap { $0 }
        lock.unlock()
        return ready
    }
}

private struct PhotoLibraryPicker: UIViewControllerRepresentable {
    @Binding var isPresented: Bool
    var onPick: ([(name: String, data: Data)], Int) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    func makeUIViewController(context: Context) -> PHPickerViewController {
        var config = PHPickerConfiguration()
        config.filter = .images
        config.selectionLimit = 0
        let picker = PHPickerViewController(configuration: config)
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ picker: PHPickerViewController, context: Context) {
        context.coordinator.parent = self
    }

    final class Coordinator: NSObject, PHPickerViewControllerDelegate {
        var parent: PhotoLibraryPicker

        init(parent: PhotoLibraryPicker) {
            self.parent = parent
        }

        func picker(_ picker: PHPickerViewController, didFinishPicking results: [PHPickerResult]) {
            parent.isPresented = false
            guard !results.isEmpty else { return }
            let onPick = parent.onPick
            let slots = PhotoSlots(count: results.count)
            let group = DispatchGroup()
            for (index, result) in results.enumerated() {
                let provider = result.itemProvider
                let identifier = provider.registeredTypeIdentifiers.first { UTType($0)?.conforms(to: .image) == true }
                    ?? UTType.image.identifier
                group.enter()
                provider.loadFileRepresentation(forTypeIdentifier: identifier) { url, _ in
                    if let url, let data = try? Data(contentsOf: url) {
                        var name = url.lastPathComponent
                        if name.isEmpty { name = "photo" }
                        if (name as NSString).pathExtension.isEmpty {
                            let ext = UTType(identifier)?.preferredFilenameExtension ?? "jpg"
                            name += ".\(ext)"
                        }
                        slots.put(index, name: name, data: data)
                    }
                    group.leave()
                }
            }
            group.notify(queue: .main) {
                let files = slots.files()
                onPick(files, results.count - files.count)
            }
        }
    }
}

/// Light matches the current cream screen. Dark matches the website's dark palette.
private struct DropPalette {
    var background: Color
    var card: Color
    var ink: Color
    var muted: Color
    var green: Color
    var onGreen: Color
    var danger: Color
    var line: Color

    static func forScheme(_ scheme: ColorScheme) -> DropPalette {
        switch scheme {
        case .dark:
            return DropPalette(
                background: Color(red: 0x12 / 255, green: 0x10 / 255, blue: 0x0d / 255),
                card: Color(red: 0x26 / 255, green: 0x21 / 255, blue: 0x1c / 255),
                ink: Color(red: 0xf6 / 255, green: 0xf1 / 255, blue: 0xe8 / 255),
                muted: Color(red: 0xd2 / 255, green: 0xc3 / 255, blue: 0xb0 / 255),
                green: Color(red: 0x7d / 255, green: 0xce / 255, blue: 0xa0 / 255),
                onGreen: Color(red: 0x10 / 255, green: 0x21 / 255, blue: 0x17 / 255),
                danger: Color(red: 0xf0 / 255, green: 0xa0 / 255, blue: 0x90 / 255),
                line: Color(red: 0x53 / 255, green: 0x48 / 255, blue: 0x38 / 255)
            )
        default:
            return DropPalette(
                background: Color(red: 0xf7 / 255, green: 0xf3 / 255, blue: 0xea / 255),
                card: Color(red: 0xff / 255, green: 0xfd / 255, blue: 0xf8 / 255),
                ink: Color(red: 0x1c / 255, green: 0x19 / 255, blue: 0x15 / 255),
                muted: Color(red: 0x6d / 255, green: 0x66 / 255, blue: 0x5c / 255),
                green: Color(red: 0x1d / 255, green: 0x68 / 255, blue: 0x43 / 255),
                onGreen: Color(red: 0xf4 / 255, green: 0xff / 255, blue: 0xf7 / 255),
                danger: Color(red: 0x9d / 255, green: 0x34 / 255, blue: 0x1c / 255),
                line: Color(red: 0xe4 / 255, green: 0xda / 255, blue: 0xc9 / 255)
            )
        }
    }
}

/// A tap on anything that is not a text field resigns first responder.
/// The recognizer lives on the window for the life of that window. A SwiftUI
/// tap gesture on the screen does not see taps the text view or scroll view
/// already claimed, and a recognizer owned by a background view leaves with it.
private struct KeyboardDismissAnchor: UIViewRepresentable {
    func makeUIView(context: Context) -> KeyboardDismissView {
        KeyboardDismissView()
    }

    func updateUIView(_ uiView: KeyboardDismissView, context: Context) {}
}

private final class KeyboardDismissView: UIView {
    override init(frame: CGRect) {
        super.init(frame: frame)
        isUserInteractionEnabled = false
        backgroundColor = .clear
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        isUserInteractionEnabled = false
        backgroundColor = .clear
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if let window {
            KeyboardDismiss.install(on: window)
        }
    }

    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        nil
    }
}

private enum KeyboardDismiss {
    private static let gestureName = "drop.keyboard-dismiss"

    static func installOnKeyWindow() {
        let scenes = UIApplication.shared.connectedScenes.compactMap { $0 as? UIWindowScene }
        let windows = scenes.flatMap(\.windows)
        if let key = windows.first(where: \.isKeyWindow) {
            install(on: key)
        }
        for window in windows where window.isKeyWindow == false && window.windowLevel == .normal {
            install(on: window)
        }
    }

    static func install(on window: UIWindow) {
        if window.gestureRecognizers?.contains(where: { $0.name == gestureName }) == true {
            return
        }
        let tap = UITapGestureRecognizer(target: Installer.shared, action: #selector(Installer.dismiss(_:)))
        tap.name = gestureName
        tap.cancelsTouchesInView = false
        tap.delaysTouchesBegan = false
        tap.delaysTouchesEnded = false
        tap.delegate = Installer.shared
        window.addGestureRecognizer(tap)
    }
}

private final class Installer: NSObject, UIGestureRecognizerDelegate {
    static let shared = Installer()

    @objc func dismiss(_ gesture: UITapGestureRecognizer) {
        guard let window = gesture.view as? UIWindow else { return }
        window.endEditing(true)
        UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
    }

    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        var view = touch.view
        while let current = view {
            if current is UITextField || current is UITextView {
                return false
            }
            view = current.superview
        }
        return true
    }

    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer,
        shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer
    ) -> Bool {
        true
    }
}
