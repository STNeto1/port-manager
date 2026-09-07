import SwiftUI
import PManagerCore

struct TunnelRowView: View {
    let snapshot: TunnelSnapshot
    let onToggle: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(dotColor)
                .frame(width: 8, height: 8)

            VStack(alignment: .leading, spacing: 2) {
                Text(snapshot.def.name)
                    .font(.system(size: 13, weight: .medium))
                Text(subtitle)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Button(action: onToggle) {
                Image(systemName: isRunning ? "stop.fill" : "play.fill")
            }
            .buttonStyle(.plain)
        }
        .padding(.vertical, 4)
        .padding(.horizontal, 8)
        .help(helpText)
    }

    private var isRunning: Bool {
        switch snapshot.state {
        case .stopped, .error:
            return false
        default:
            return true
        }
    }

    private var subtitle: String {
        "\(snapshot.def.profile) · \(snapshot.def.localBind.bindAddr):\(snapshot.def.localBind.port)"
    }

    private var dotColor: Color {
        switch snapshot.state {
        case .connected: return .green
        case .error: return .red
        case .connecting, .stopping: return .orange
        case .stopped: return .gray
        }
    }

    private var helpText: String {
        if case .error(let message) = snapshot.state {
            return message
        }
        return snapshot.def.name
    }
}
