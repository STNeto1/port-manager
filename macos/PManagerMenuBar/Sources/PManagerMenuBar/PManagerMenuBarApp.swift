import SwiftUI
import PManagerCore

@main
struct PManagerMenuBarApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @StateObject private var viewModel = MenuBarViewModel()

    var body: some Scene {
        MenuBarExtra {
            TunnelListContentView(viewModel: viewModel)
        } label: {
            MenuBarIconView(status: viewModel.aggregateStatus)
        }
        .menuBarExtraStyle(.window)
    }
}

/// Built as a plain SPM executable rather than an .app bundle (see macos/PManagerMenuBar/README.md),
/// so there's no Info.plist to set LSUIElement — hide the Dock icon/app-switcher entry programmatically instead.
final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
    }
}
