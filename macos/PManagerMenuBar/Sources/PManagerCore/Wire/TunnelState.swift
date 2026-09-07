import Foundation

/// Mirrors `TunnelState` in src/model.rs — serde's default externally-tagged
/// shape: unit variants as bare strings, `Connected` as a nested struct,
/// `Error` as a nested string. Decode-only: the daemon never receives one.
public enum TunnelState: Decodable {
    case stopped
    case connecting
    case connected(activeConnections: UInt32)
    case error(String)
    case stopping

    private enum CodingKeys: String, CodingKey {
        case connected = "Connected"
        case error = "Error"
    }

    private enum ConnectedKeys: String, CodingKey {
        case activeConnections = "active_connections"
    }

    public init(from decoder: Decoder) throws {
        if let single = try? decoder.singleValueContainer(), let raw = try? single.decode(String.self) {
            switch raw {
            case "Stopped":
                self = .stopped
                return
            case "Connecting":
                self = .connecting
                return
            case "Stopping":
                self = .stopping
                return
            default:
                break
            }
        }

        let container = try decoder.container(keyedBy: CodingKeys.self)
        if container.contains(.connected) {
            let nested = try container.nestedContainer(keyedBy: ConnectedKeys.self, forKey: .connected)
            let count = try nested.decode(UInt32.self, forKey: .activeConnections)
            self = .connected(activeConnections: count)
            return
        }
        if container.contains(.error) {
            let message = try container.decode(String.self, forKey: .error)
            self = .error(message)
            return
        }
        throw DecodingError.dataCorruptedError(
            forKey: .connected, in: container, debugDescription: "Unknown TunnelState shape")
    }
}
