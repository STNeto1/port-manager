use pmanager::config::schema::{AuthMethod, Direction, SocketAddrSpec, TunnelDefinition};
use pmanager::ipc::protocol::{ClientMessage, ClientRequest, DaemonMessage, ResponsePayload, TunnelSnapshot};
use pmanager::model::TunnelState;
use uuid::Uuid;

#[test]
fn client_message_round_trips() {
    let msg = ClientMessage {
        request_id: 42,
        request: ClientRequest::ListTunnels,
    };
    let json = serde_json::to_string(&msg).unwrap();
    let decoded: ClientMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.request_id, 42);
    assert!(matches!(decoded.request, ClientRequest::ListTunnels));
}

#[test]
fn daemon_response_with_tunnel_list_round_trips() {
    let snapshot = TunnelSnapshot {
        def: TunnelDefinition {
            id: Uuid::new_v4(),
            name: "example".to_string(),
            direction: Direction::Local,
            host: "bastion.example.com".to_string(),
            port: 22,
            username: "deploy".to_string(),
            auth: AuthMethod::Agent,
            jump: None,
            local_bind: SocketAddrSpec {
                bind_addr: "127.0.0.1".to_string(),
                port: 15432,
            },
            remote: None,
            autostart: false,
            enabled: true,
        },
        state: TunnelState::Connected {
            active_connections: 2,
        },
    };

    let msg = DaemonMessage::Response {
        request_id: 7,
        result: Ok(ResponsePayload::Tunnels(vec![snapshot])),
    };

    let json = serde_json::to_string(&msg).unwrap();
    let decoded: DaemonMessage = serde_json::from_str(&json).unwrap();

    let DaemonMessage::Response { request_id, result } = decoded;
    assert_eq!(request_id, 7);
    let ResponsePayload::Tunnels(tunnels) = result.unwrap() else {
        panic!("expected Tunnels payload");
    };
    assert_eq!(tunnels.len(), 1);
    assert_eq!(tunnels[0].def.name, "example");
    assert!(matches!(
        tunnels[0].state,
        TunnelState::Connected { active_connections: 2 }
    ));
}

#[test]
fn error_response_round_trips() {
    let msg = DaemonMessage::Response {
        request_id: 1,
        result: Err("something went wrong".to_string()),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let decoded: DaemonMessage = serde_json::from_str(&json).unwrap();

    let DaemonMessage::Response { result, .. } = decoded;
    assert_eq!(result.unwrap_err(), "something went wrong");
}
