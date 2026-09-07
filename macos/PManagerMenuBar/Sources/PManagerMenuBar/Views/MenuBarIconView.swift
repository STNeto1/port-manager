import SwiftUI
import PManagerCore

struct MenuBarIconView: View {
    let status: MenuBarViewModel.AggregateStatus

    var body: some View {
        Image(systemName: symbolName)
            .foregroundStyle(color)
            .opacity(status == .disconnected ? 0.4 : 1.0)
    }

    private var symbolName: String {
        switch status {
        case .error: return "exclamationmark.triangle.fill"
        default: return "network"
        }
    }

    private var color: Color {
        switch status {
        case .error: return .red
        case .connected: return .green
        case .transitioning: return .orange
        case .idle: return .primary
        case .disconnected: return .secondary
        }
    }
}
