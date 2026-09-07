import SwiftUI
import PManagerCore

struct DisconnectedFooterView: View {
    let onRetry: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Daemon not running")
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
            Button("Retry", action: onRetry)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
    }
}
