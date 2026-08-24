pub mod app;
pub mod event;
#[allow(clippy::module_inception)]
// tui.rs (terminal init/restore) inside client::tui is the standard ratatui-template layout
pub mod tui;
pub mod ui;

use std::time::Duration;

use color_eyre::eyre::Result;
use tokio::sync::mpsc::UnboundedSender;

use app::{Action, App, FormState, Mode};
use event::{AppEvent, EventHandler};

use crate::client::{DaemonClient, connect_or_spawn_daemon};
use crate::ipc::protocol::{ClientRequest, ResponsePayload};
use crate::model::TunnelState;

pub async fn run() -> Result<()> {
    let mut app = App::new();
    let mut client = connect_or_spawn_daemon().await.ok();

    if let Some(client) = client.as_mut() {
        app.daemon_connected = true;
        refresh_tunnels(&mut app, client).await;
    }

    let mut terminal = tui::init()?;
    let mut events = EventHandler::new(Duration::from_millis(200));
    spawn_event_subscription(events.sender.clone());

    let result = main_loop(&mut terminal, &mut app, &mut events, &mut client).await;

    tui::restore()?;
    result
}

/// A dedicated connection, separate from the request/response one held by
/// the main loop, that subscribes once and then only ever reads pushed
/// `TunnelEvent`s — fed into the same AppEvent channel as Tick/Key/Resize.
fn spawn_event_subscription(sender: UnboundedSender<AppEvent>) {
    tokio::spawn(async move {
        let Ok(mut client) = connect_or_spawn_daemon().await else {
            return;
        };
        if client.call(ClientRequest::Subscribe).await.is_err() {
            return;
        }
        while let Ok(Some(event)) = client.recv_event().await {
            if sender.send(AppEvent::Tunnel(event)).is_err() {
                break;
            }
        }
    });
}

async fn main_loop(
    terminal: &mut tui::Tui,
    app: &mut App,
    events: &mut EventHandler,
    client: &mut Option<DaemonClient>,
) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;

        let event = match events.receiver.recv().await {
            Some(event) => event,
            None => return Ok(()),
        };

        match event {
            AppEvent::Tick => {}
            AppEvent::Resize(_, _) => {}
            AppEvent::Tunnel(tunnel_event) => app.apply_tunnel_event(tunnel_event),
            AppEvent::Key(key) => {
                let action = app.handle_key(key);
                match client.as_mut() {
                    Some(client) => apply_action(app, client, action).await,
                    None if matches!(action, Action::Quit) => app.should_quit = true,
                    None => {}
                }
            }
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

async fn apply_action(app: &mut App, client: &mut DaemonClient, action: Action) {
    match action {
        Action::None => {}
        Action::Quit => app.should_quit = true,
        Action::OpenAddForm => app.mode = Mode::Form(Box::new(FormState::new_add())),
        Action::OpenEditForm => {
            if let Some(def) = app.selected_snapshot().map(|s| s.def.clone()) {
                app.mode = Mode::Form(Box::new(FormState::from_definition(&def)));
            }
        }
        Action::OpenDeleteConfirm => {
            if let Some(id) = app.selected_snapshot().map(|s| s.def.id) {
                app.mode = Mode::ConfirmDelete(id);
            }
        }
        Action::CancelForm | Action::CancelDelete => app.mode = Mode::List,
        Action::SubmitForm => submit_form(app, client).await,
        Action::ConfirmDelete => confirm_delete(app, client).await,
        Action::ToggleStartStop => toggle_start_stop(app, client).await,
        Action::Reload => {
            let _ = client.call(ClientRequest::ReloadConfig).await;
            refresh_tunnels(app, client).await;
        }
    }
}

async fn submit_form(app: &mut App, client: &mut DaemonClient) {
    let Mode::Form(form) = &app.mode else { return };
    let definition = form.build_definition();
    let is_edit = form.editing_id.is_some();

    match definition {
        Ok(definition) => {
            let request = if is_edit {
                ClientRequest::UpdateTunnel(definition)
            } else {
                ClientRequest::AddTunnel(definition)
            };
            match client.call(request).await {
                Ok(_) => {
                    refresh_tunnels(app, client).await;
                    app.mode = Mode::List;
                }
                Err(err) => set_form_error(app, err.to_string()),
            }
        }
        Err(err) => set_form_error(app, err),
    }
}

fn set_form_error(app: &mut App, error: String) {
    if let Mode::Form(form) = &mut app.mode {
        form.error = Some(error);
    }
}

async fn confirm_delete(app: &mut App, client: &mut DaemonClient) {
    let Mode::ConfirmDelete(id) = &app.mode else {
        return;
    };
    let id = *id;
    let _ = client.call(ClientRequest::RemoveTunnel(id)).await;
    refresh_tunnels(app, client).await;
    app.mode = Mode::List;
}

async fn toggle_start_stop(app: &mut App, client: &mut DaemonClient) {
    let Some(snapshot) = app.selected_snapshot() else {
        return;
    };
    let id = snapshot.def.id;
    let request = match &snapshot.state {
        TunnelState::Stopped | TunnelState::Error(_) => ClientRequest::StartTunnel(id),
        _ => ClientRequest::StopTunnel(id),
    };
    let _ = client.call(request).await;
}

async fn refresh_tunnels(app: &mut App, client: &mut DaemonClient) {
    if let Ok(ResponsePayload::Tunnels(tunnels)) = client.call(ClientRequest::ListTunnels).await {
        app.tunnels = tunnels;
        match app.table_state.selected() {
            Some(i) if i >= app.tunnels.len() => {
                app.table_state.select(if app.tunnels.is_empty() {
                    None
                } else {
                    Some(app.tunnels.len() - 1)
                });
            }
            None if !app.tunnels.is_empty() => app.table_state.select(Some(0)),
            _ => {}
        }
    }
}
