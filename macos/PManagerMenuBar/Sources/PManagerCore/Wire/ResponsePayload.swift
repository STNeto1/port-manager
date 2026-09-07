import Foundation

/// Mirrors `TunnelSnapshot` in src/ipc/protocol.rs.
public struct TunnelSnapshot: Decodable, Identifiable {
    public var id: UUID { def.id }
    public let def: TunnelDefinition
    public let state: TunnelState

    public init(def: TunnelDefinition, state: TunnelState) {
        self.def = def
        self.state = state
    }
}

/// Mirrors `ResponsePayload` in src/ipc/protocol.rs. Decode-only: the client
/// only ever receives one, never constructs it.
public enum ResponsePayload: Decodable {
    case ack
    case tunnels([TunnelSnapshot])
    case profiles([Profile])

    private enum CodingKeys: String, CodingKey {
        case tunnels = "Tunnels"
        case profiles = "Profiles"
    }

    public init(from decoder: Decoder) throws {
        if let single = try? decoder.singleValueContainer(), let raw = try? single.decode(String.self), raw == "Ack" {
            self = .ack
            return
        }

        let container = try decoder.container(keyedBy: CodingKeys.self)
        if container.contains(.tunnels) {
            self = .tunnels(try container.decode([TunnelSnapshot].self, forKey: .tunnels))
            return
        }
        if container.contains(.profiles) {
            self = .profiles(try container.decode([Profile].self, forKey: .profiles))
            return
        }
        throw DecodingError.dataCorruptedError(
            forKey: .tunnels, in: container, debugDescription: "Unknown ResponsePayload shape")
    }
}
