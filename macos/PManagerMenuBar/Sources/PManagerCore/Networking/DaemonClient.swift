import Foundation

/// One request/response connection to the daemon, mirroring `DaemonClient`
/// in src/client/mod.rs: an incrementing `request_id` per connection, send
/// then read frames until the matching `Response` arrives (discarding stray
/// frames in between).
///
/// Swift actors are reentrant across `await` suspension points, unlike the
/// Rust code's implicit single-request-at-a-time guarantee on one socket —
/// two concurrent `call()`s (e.g. rapid button clicks) could otherwise
/// interleave reads and steal each other's response frames. `acquire()`/
/// `release()` serialize `call()` end-to-end to prevent that.
public actor DaemonClient {
    private var connection: UnixSocketConnection?
    private var lineIterator: AsyncThrowingStream<Data, Error>.Iterator?
    private var nextRequestId: UInt64 = 0

    private var inFlight = false
    private var waiters: [CheckedContinuation<Void, Never>] = []

    public init() {}

    public func connectOrSpawn() async throws {
        if connection != nil { return }
        do {
            try await connectOnly()
        } catch {
            DaemonProcess.spawnDaemonIfPossible()
            try await waitForDaemon()
        }
    }

    private func connectOnly() async throws {
        let conn = UnixSocketConnection(path: PManagerPaths.socketPath)
        try await conn.start()
        connection = conn
        lineIterator = conn.lines().makeAsyncIterator()
    }

    private func waitForDaemon() async throws {
        let deadline = Date().addingTimeInterval(3)
        while Date() < deadline {
            do {
                try await connectOnly()
                return
            } catch {
                try await Task.sleep(nanoseconds: 100_000_000)
            }
        }
        throw DaemonError(message: "Could not connect to the pmanager daemon")
    }

    public func call(_ request: ClientRequest) async throws -> ResponsePayload {
        await acquire()
        defer { release() }

        guard let connection, var iterator = lineIterator else {
            throw DaemonError(message: "Not connected")
        }

        let requestId = nextRequestId
        nextRequestId += 1

        let message = ClientMessage(requestId: requestId, request: request)
        let data = try JSONEncoder().encode(message)
        guard let line = String(data: data, encoding: .utf8) else {
            throw DaemonError(message: "Failed to encode request")
        }
        try await connection.send(line: line)

        while true {
            guard let lineData = try await iterator.next() else {
                lineIterator = iterator
                throw DaemonError(message: "Connection closed while waiting for a response")
            }
            lineIterator = iterator

            let decoded = try JSONDecoder().decode(DaemonMessage.self, from: lineData)
            if case .response(let rid, let result) = decoded, rid == requestId {
                switch result {
                case .success(let payload): return payload
                case .failure(let error): throw error
                }
            }
        }
    }

    public func disconnect() {
        connection?.cancel()
        connection = nil
        lineIterator = nil
    }

    private func acquire() async {
        if !inFlight {
            inFlight = true
            return
        }
        await withCheckedContinuation { continuation in
            waiters.append(continuation)
        }
    }

    private func release() {
        if waiters.isEmpty {
            inFlight = false
        } else {
            waiters.removeFirst().resume()
        }
    }
}
