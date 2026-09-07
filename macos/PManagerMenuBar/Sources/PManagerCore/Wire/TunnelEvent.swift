import Foundation

/// Mirrors `TunnelEvent` in src/model.rs. `StateChanged` is a Rust tuple
/// variant with two fields, which serde serializes as a JSON array —
/// `{"StateChanged": ["<uuid>", <TunnelState>]}` — not a keyed object.
public enum TunnelEvent: Decodable {
    case stateChanged(id: UUID, state: TunnelState)

    private enum CodingKeys: String, CodingKey {
        case stateChanged = "StateChanged"
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        var unkeyed = try container.nestedUnkeyedContainer(forKey: .stateChanged)
        let idString = try unkeyed.decode(String.self)
        guard let uuid = UUID(uuidString: idString) else {
            throw DecodingError.dataCorruptedError(
                in: unkeyed, debugDescription: "Invalid UUID in StateChanged: \(idString)")
        }
        let state = try unkeyed.decode(TunnelState.self)
        self = .stateChanged(id: uuid, state: state)
    }
}
