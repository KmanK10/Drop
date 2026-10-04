import SwiftUI

@main
struct DropApp: App {
    @StateObject private var model = SessionModel()

    var body: some Scene {
        WindowGroup {
            RootView()
                .environmentObject(model)
                .onOpenURL { _ in
                    model.drainInbox()
                }
        }
    }
}
