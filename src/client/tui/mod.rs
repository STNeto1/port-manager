pub mod app;
pub mod event;
pub mod tui;
pub mod ui;

use std::time::Duration;

use color_eyre::eyre::Result;

use app::App;
use event::{AppEvent, EventHandler};

use crate::client::connect_or_spawn_daemon;
use crate::ipc::protocol::ClientRequest;

pub async fn run() -> Result<()> {
    let mut app = App::new();
    app.daemon_connected = match connect_or_spawn_daemon().await {
        Ok(mut client) => client.call(ClientRequest::Ping).await.is_ok(),
        Err(_) => false,
    };

    let mut terminal = tui::init()?;
    let mut events = EventHandler::new(Duration::from_millis(200));

    let result = main_loop(&mut terminal, &mut app, &mut events).await;

    tui::restore()?;
    result
}

async fn main_loop(terminal: &mut tui::Tui, app: &mut App, events: &mut EventHandler) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;

        match events.receiver.recv().await {
            Some(event) => apply_event(app, event),
            None => return Ok(()),
        }

        if app.should_quit {
            return Ok(());
        }
    }
}

fn apply_event(app: &mut App, event: AppEvent) {
    match event {
        AppEvent::Tick => {}
        AppEvent::Key(key) => app.handle_key(key),
        AppEvent::Resize(_, _) => {}
    }
}
