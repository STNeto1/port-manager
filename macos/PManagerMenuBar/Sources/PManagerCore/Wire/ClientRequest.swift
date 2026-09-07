import Foundation

/// Mirrors the subset of `ClientRequest` (src/ipc/protocol.rs) this v1 menu
/// bar client needs. Encode-only. Unit variants encode as bare strings;
/// single-payload variants encode as a one-key object keyed by the Rust
/// variant name, matching serde's default externally-tagged representation.
public enum ClientRequest: Encodable {
    case listTunnels
    case listProfiles
    case subscribe
    case startTunnel(UUID)
    case stopTunnel(UUID)

    private struct VariantKey: CodingKey {
        var stringValue: String
        init(stringValue: String) { self.stringValue = stringValue }
        var intValue: Int? { nil }
        init?(intValue: Int) { nil }
    }

    public func encode(to encoder: Encoder) throws {
        switch self {
        case .listTunnels:
            var single = encoder.singleValueContainer()
            try single.encode("ListTunnels")
        case .listProfiles:
            var single = encoder.singleValueContainer()
            try single.encode("ListProfiles")
        case .subscribe:
            var single = encoder.singleValueContainer()
            try single.encode("Subscribe")
        case .startTunnel(let id):
            var container = encoder.container(keyedBy: VariantKey.self)
            try container.encode(id.uuidString.lowercased(), forKey: VariantKey(stringValue: "StartTunnel"))
        case .stopTunnel(let id):
            var container = encoder.container(keyedBy: VariantKey.self)
            try container.encode(id.uuidString.lowercased(), forKey: VariantKey(stringValue: "StopTunnel"))
        }
    }
}
