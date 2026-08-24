use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub struct App {
    pub should_quit: bool,
    pub daemon_connected: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            should_quit: false,
            daemon_connected: true,
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
