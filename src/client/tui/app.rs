use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::ipc::protocol::TunnelSnapshot;

pub struct App {
    pub should_quit: bool,
    pub daemon_connected: bool,
    pub tunnels: Vec<TunnelSnapshot>,
}

impl App {
    pub fn new() -> Self {
        Self {
            should_quit: false,
            daemon_connected: false,
            tunnels: Vec::new(),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            _ => {}
        }
    }
}
