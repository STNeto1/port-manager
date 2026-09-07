import Foundation
import Network

/// A single Unix-domain-socket connection framed as newline-delimited JSON,
/// mirroring `tokio_util::codec::LinesCodec` on the daemon side: one JSON
/// object plus `\n` per message, in either direction.
final class UnixSocketConnection {
    private let connection: NWConnection
    private let queue = DispatchQueue(label: "pmanager.unixsocket")
    private var buffer = Data()

    init(path: String) {
        let endpoint = NWEndpoint.unix(path: path)
        connection = NWConnection(to: endpoint, using: .tcp)
    }

    private final class ResumeOnce: @unchecked Sendable {
        private let lock = NSLock()
        private var resumed = false

        func run(_ body: () -> Void) {
            lock.lock()
            defer { lock.unlock() }
            guard !resumed else { return }
            resumed = true
            body()
        }
    }

    func start() async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            let resumeOnce = ResumeOnce()
            connection.stateUpdateHandler = { state in
                switch state {
                case .ready:
                    resumeOnce.run { continuation.resume() }
                case .failed(let error):
                    resumeOnce.run { continuation.resume(throwing: error) }
                case .cancelled:
                    resumeOnce.run { continuation.resume(throwing: DaemonError(message: "Connection cancelled")) }
                case .waiting(let error):
                    // Network.framework's default recovery behavior: an
                    // unreachable Unix socket path (nothing listening yet)
                    // lands here, not `.failed`, and would otherwise retry
                    // forever on its own schedule. Treat it as a failure —
                    // the caller (DaemonClient) already does its own bounded
                    // connect-retry loop.
                    resumeOnce.run {
                        self.connection.cancel()
                        continuation.resume(throwing: error)
                    }
                default:
                    break
                }
            }
            connection.start(queue: queue)
        }
    }

    func send(line: String) async throws {
        var data = Data(line.utf8)
        data.append(0x0A)
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            connection.send(
                content: data,
                completion: .contentProcessed { error in
                    if let error {
                        continuation.resume(throwing: error)
                    } else {
                        continuation.resume()
                    }
                })
        }
    }

    /// One element per `\n`-delimited line received (the line's bytes,
    /// without the newline).
    func lines() -> AsyncThrowingStream<Data, Error> {
        AsyncThrowingStream { continuation in
            self.receiveLoop(continuation: continuation)
        }
    }

    private func receiveLoop(continuation: AsyncThrowingStream<Data, Error>.Continuation) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 64 * 1024) { [weak self] data, _, isComplete, error in
            guard let self else { return }
            if let data, !data.isEmpty {
                self.buffer.append(data)
                while let newlineIndex = self.buffer.firstIndex(of: 0x0A) {
                    let lineData = self.buffer.subdata(in: self.buffer.startIndex..<newlineIndex)
                    self.buffer.removeSubrange(self.buffer.startIndex...newlineIndex)
                    continuation.yield(lineData)
                }
            }
            if let error {
                continuation.finish(throwing: error)
                return
            }
            if isComplete {
                continuation.finish()
                return
            }
            self.receiveLoop(continuation: continuation)
        }
    }

    func cancel() {
        connection.cancel()
    }
}
