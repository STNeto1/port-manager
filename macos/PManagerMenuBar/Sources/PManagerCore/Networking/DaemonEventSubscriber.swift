import Foundation

/// A second, dedicated connection to the daemon that does nothing but send
/// `Subscribe` once and then read pushed `TunnelEvent`s forever.
///
/// This mirrors `spawn_event_subscription` in src/client/tui/mod.rs and the
/// daemon's documented semantic that `Subscribe` permanently converts a
/// connection into event-only mode — after the `Ack`, the daemon never
/// accepts further requests on this socket. Callers must never call a
/// request/response method on this connection; use a separate `DaemonClient`
/// for that.
public final class DaemonEventSubscriber {
    private var connection: UnixSocketConnection?

    public init() {}

    /// Assumes the daemon is already reachable (the caller should have
    /// already run `DaemonClient.connectOrSpawn()` once) — this subscriber
    /// does not itself spawn the daemon.
    public func events() -> AsyncThrowingStream<TunnelEvent, Error> {
        AsyncThrowingStream { continuation in
            Task {
                do {
                    let conn = UnixSocketConnection(path: PManagerPaths.socketPath)
                    try await conn.start()
                    self.connection = conn

                    var iterator = conn.lines().makeAsyncIterator()

                    let subscribeMessage = ClientMessage(requestId: 0, request: .subscribe)
                    let data = try JSONEncoder().encode(subscribeMessage)
                    guard let line = String(data: data, encoding: .utf8) else {
                        throw DaemonError(message: "Failed to encode subscribe request")
                    }
                    try await conn.send(line: line)

                    guard let ackLine = try await iterator.next() else {
                        throw DaemonError(message: "Connection closed before Ack")
                    }
                    let ackMessage = try JSONDecoder().decode(DaemonMessage.self, from: ackLine)
                    guard case .response(_, .success) = ackMessage else {
                        throw DaemonError(message: "Unexpected response to Subscribe")
                    }

                    while let lineData = try await iterator.next() {
                        let message = try JSONDecoder().decode(DaemonMessage.self, from: lineData)
                        if case .event(let event) = message {
                            continuation.yield(event)
                        }
                    }
                    continuation.finish()
                } catch {
                    continuation.finish(throwing: error)
                }
            }
        }
    }

    public func cancel() {
        connection?.cancel()
    }
}
