import Foundation

/// Mirrors `Direction` in src/config/schema.rs — a snake_case bare-string enum.
public enum Direction: String, Codable {
    case local
    case remote
    case dynamic
}

/// Mirrors `HostPort` in src/config/schema.rs.
public struct HostPort: Codable {
    public let host: String
    public let port: UInt16
}

/// Mirrors `SocketAddrSpec` in src/config/schema.rs.
public struct SocketAddrSpec: Codable {
    public let bindAddr: String
    public let port: UInt16

    enum CodingKeys: String, CodingKey {
        case bindAddr = "bind_addr"
        case port
    }
}

/// Mirrors `AuthMethod` in src/config/schema.rs — the one internally-tagged
/// type on the wire (`#[serde(tag = "type")]`), so it decodes/encodes a flat
/// `type` discriminant alongside its fields rather than a wrapper key.
public enum AuthMethod: Codable {
    case password(password: String?)
    case privateKey(path: String, passphrase: String?)
    case agent

    private enum CodingKeys: String, CodingKey {
        case type
        case password
        case path
        case passphrase
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let type = try container.decode(String.self, forKey: .type)
        switch type {
        case "password":
            let password = try container.decodeIfPresent(String.self, forKey: .password)
            self = .password(password: password)
        case "private_key":
            let path = try container.decode(String.self, forKey: .path)
            let passphrase = try container.decodeIfPresent(String.self, forKey: .passphrase)
            self = .privateKey(path: path, passphrase: passphrase)
        case "agent":
            self = .agent
        default:
            throw DecodingError.dataCorruptedError(
                forKey: .type, in: container, debugDescription: "Unknown auth method type: \(type)")
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        switch self {
        case .password(let password):
            try container.encode("password", forKey: .type)
            try container.encodeIfPresent(password, forKey: .password)
        case .privateKey(let path, let passphrase):
            try container.encode("private_key", forKey: .type)
            try container.encode(path, forKey: .path)
            try container.encodeIfPresent(passphrase, forKey: .passphrase)
        case .agent:
            try container.encode("agent", forKey: .type)
        }
    }
}

/// Mirrors `Profile` in src/config/schema.rs.
public struct Profile: Codable, Identifiable {
    public var id: String { name }
    public let name: String
    public let host: String
    public let port: UInt16
    public let username: String
    public let auth: AuthMethod
    public let jump: String?
}

/// Mirrors `TunnelDefinition` in src/config/schema.rs.
public struct TunnelDefinition: Codable, Identifiable {
    public let id: UUID
    public let name: String
    public let direction: Direction
    public let profile: String
    public let localBind: SocketAddrSpec
    public let remote: HostPort?
    public let autostart: Bool
    public let enabled: Bool

    enum CodingKeys: String, CodingKey {
        case id, name, direction, profile
        case localBind = "local_bind"
        case remote, autostart, enabled
    }
}
