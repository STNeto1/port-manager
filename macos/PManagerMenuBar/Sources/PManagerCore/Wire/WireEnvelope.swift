import Foundation

/// An error message returned by the daemon (the `Err` side of the Rust
/// `Result<ResponsePayload, String>`), or raised locally by the client.
struct DaemonError: Error, LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

/// Mirrors `ClientMessage` in src/ipc/protocol.rs. Encode-only.
struct ClientMessage: Encodable {
    let requestId: UInt64
    let request: ClientRequest

    enum CodingKeys: String, CodingKey {
        case requestId = "request_id"
        case request
    }
}

/// Mirrors `DaemonMessage` in src/ipc/protocol.rs. Decode-only. `Response`'s
/// `result` field is serde's default `Result<T, String>` shape:
/// `{"Ok": <ResponsePayload>}` or `{"Err": "<message>"}`.
enum DaemonMessage: Decodable {
    case response(requestId: UInt64, result: Result<ResponsePayload, DaemonError>)
    case event(TunnelEvent)

    private enum CodingKeys: String, CodingKey {
        case response = "Response"
        case event = "Event"
    }

    private enum ResponseKeys: String, CodingKey {
        case requestId = "request_id"
        case result
    }

    private enum ResultKeys: String, CodingKey {
        case ok = "Ok"
        case err = "Err"
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)

        if container.contains(.response) {
            let nested = try container.nestedContainer(keyedBy: ResponseKeys.self, forKey: .response)
            let requestId = try nested.decode(UInt64.self, forKey: .requestId)
            let resultContainer = try nested.nestedContainer(keyedBy: ResultKeys.self, forKey: .result)

            if resultContainer.contains(.ok) {
                let payload = try resultContainer.decode(ResponsePayload.self, forKey: .ok)
                self = .response(requestId: requestId, result: .success(payload))
                return
            }
            if resultContainer.contains(.err) {
                let message = try resultContainer.decode(String.self, forKey: .err)
                self = .response(requestId: requestId, result: .failure(DaemonError(message: message)))
                return
            }
            throw DecodingError.dataCorruptedError(
                forKey: .ok, in: resultContainer, debugDescription: "Unknown Result shape")
        }

        if container.contains(.event) {
            let event = try container.decode(TunnelEvent.self, forKey: .event)
            self = .event(event)
            return
        }

        throw DecodingError.dataCorruptedError(
            forKey: .response, in: container, debugDescription: "Unknown DaemonMessage shape")
    }
}
