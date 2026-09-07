import Foundation

@MainActor
public final class MenuBarViewModel: ObservableObject {
    public enum ConnectionState: Equatable {
        case connecting
        case connected
        case disconnected
    }

    public enum AggregateStatus: Equatable {
        case error
        case connected
        case transitioning
        case idle
        case disconnected
    }

    @Published public private(set) var tunnels: [TunnelSnapshot] = []
    @Published public private(set) var connectionState: ConnectionState = .connecting

    public var aggregateStatus: AggregateStatus {
        guard connectionState == .connected else { return .disconnected }

        if tunnels.contains(where: { if case .error = $0.state { return true }; return false }) {
            return .error
        }
        if tunnels.contains(where: { if case .connected = $0.state { return true }; return false }) {
            return .connected
        }
        if tunnels.contains(where: {
            switch $0.state {
            case .connecting, .stopping: return true
            default: return false
            }
        }) {
            return .transitioning
        }
        return .idle
    }

    private let client = DaemonClient()
    private var retryTask: Task<Void, Never>?

    public init() {}

    public func start() {
        guard retryTask == nil else { return }
        retryTask = Task { [weak self] in
            await self?.connectLoop()
        }
    }

    public func retryNow() {
        retryTask?.cancel()
        retryTask = nil
        start()
    }

    private func connectLoop() async {
        while !Task.isCancelled {
            connectionState = .connecting
            do {
                try await client.connectOrSpawn()
                let payload = try await client.call(.listTunnels)
                if case .tunnels(let snapshots) = payload {
                    tunnels = snapshots.sorted { $0.def.name < $1.def.name }
                }
                connectionState = .connected
                await runEventLoop()
            } catch {
                connectionState = .disconnected
            }

            if Task.isCancelled { return }
            try? await Task.sleep(nanoseconds: 2_000_000_000)
        }
    }

    private func runEventLoop() async {
        let subscriber = DaemonEventSubscriber()
        do {
            for try await event in subscriber.events() {
                switch event {
                case .stateChanged(let id, let state):
                    if let index = tunnels.firstIndex(where: { $0.def.id == id }) {
                        tunnels[index] = TunnelSnapshot(def: tunnels[index].def, state: state)
                    }
                }
            }
        } catch {
            // Falls through to mark disconnected and retry below.
        }
        connectionState = .disconnected
    }

    public func toggle(_ snapshot: TunnelSnapshot) {
        let shouldStart: Bool
        switch snapshot.state {
        case .stopped, .error:
            shouldStart = true
        default:
            shouldStart = false
        }

        Task { [client] in
            do {
                _ = try await client.call(shouldStart ? .startTunnel(snapshot.def.id) : .stopTunnel(snapshot.def.id))
            } catch {
                // Best effort — the event stream will reconcile actual state.
            }
        }
    }
}
