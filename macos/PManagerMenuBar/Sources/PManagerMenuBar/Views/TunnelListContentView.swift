import SwiftUI
import PManagerCore

struct TunnelListContentView: View {
    @ObservedObject var viewModel: MenuBarViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Text("pmanager")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(.secondary)
                .padding(.horizontal, 12)
                .padding(.top, 10)
                .padding(.bottom, 4)

            content

            Divider()
                .padding(.vertical, 4)

            Button("Quit pmanager") {
                NSApplication.shared.terminate(nil)
            }
            .buttonStyle(.plain)
            .padding(.horizontal, 12)
            .padding(.bottom, 10)
        }
        .frame(width: 260)
        .task {
            viewModel.start()
        }
    }

    @ViewBuilder
    private var content: some View {
        switch viewModel.connectionState {
        case .connected:
            if viewModel.tunnels.isEmpty {
                placeholder("No tunnels configured")
            } else {
                ForEach(viewModel.tunnels) { snapshot in
                    TunnelRowView(snapshot: snapshot) {
                        viewModel.toggle(snapshot)
                    }
                    .padding(.horizontal, 4)
                }
            }
        case .connecting:
            placeholder("Connecting…")
        case .disconnected:
            DisconnectedFooterView {
                viewModel.retryNow()
            }
        }
    }

    private func placeholder(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 12))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
    }
}
