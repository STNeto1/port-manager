use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;
use uuid::Uuid;

use crate::config::schema::{Direction, HostPort, Profile, SocketAddrSpec, TunnelDefinition};
use crate::ipc::protocol::TunnelSnapshot;
use crate::model::TunnelEvent;

pub struct App {
    pub should_quit: bool,
    pub daemon_connected: bool,
    pub tunnels: Vec<TunnelSnapshot>,
    pub profiles: Vec<Profile>,
    pub table_state: TableState,
    pub mode: Mode,
}

pub enum Mode {
    List,
    Form(Box<FormState>),
    ConfirmDelete(Uuid),
}

/// What a keypress should trigger; the actual daemon call happens in the
/// async main loop, which is why App itself stays synchronous.
pub enum Action {
    None,
    Quit,
    OpenAddForm,
    OpenEditForm,
    OpenDeleteConfirm,
    CancelForm,
    CancelDelete,
    SubmitForm,
    ConfirmDelete,
    ToggleStartStop,
    Reload,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            should_quit: false,
            daemon_connected: false,
            tunnels: Vec::new(),
            profiles: Vec::new(),
            table_state: TableState::default(),
            mode: Mode::List,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        match &mut self.mode {
            Mode::List => self.handle_list_key(key),
            Mode::Form(form) => handle_form_key(form, key, &self.profiles),
            Mode::ConfirmDelete(_) => handle_confirm_key(key),
        }
    }

    fn handle_list_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_next();
                Action::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_previous();
                Action::None
            }
            KeyCode::Enter | KeyCode::Char('s') => Action::ToggleStartStop,
            KeyCode::Char('a') => Action::OpenAddForm,
            KeyCode::Char('e') => Action::OpenEditForm,
            KeyCode::Char('d') => Action::OpenDeleteConfirm,
            KeyCode::Char('r') => Action::Reload,
            _ => Action::None,
        }
    }

    pub fn select_next(&mut self) {
        if self.tunnels.is_empty() {
            return;
        }
        let next = match self.table_state.selected() {
            Some(i) if i + 1 < self.tunnels.len() => i + 1,
            _ => 0,
        };
        self.table_state.select(Some(next));
    }

    pub fn select_previous(&mut self) {
        if self.tunnels.is_empty() {
            return;
        }
        let prev = match self.table_state.selected() {
            Some(0) | None => self.tunnels.len() - 1,
            Some(i) => i - 1,
        };
        self.table_state.select(Some(prev));
    }

    pub fn selected_snapshot(&self) -> Option<&TunnelSnapshot> {
        self.table_state
            .selected()
            .and_then(|i| self.tunnels.get(i))
    }

    pub fn apply_tunnel_event(&mut self, event: TunnelEvent) {
        match event {
            TunnelEvent::StateChanged(id, state) => {
                if let Some(snapshot) = self.tunnels.iter_mut().find(|t| t.def.id == id) {
                    snapshot.state = state;
                }
            }
        }
    }
}

fn handle_confirm_key(key: KeyEvent) -> Action {
    match key.code {
        KeyCode::Char('y') | KeyCode::Enter => Action::ConfirmDelete,
        KeyCode::Char('n') | KeyCode::Esc => Action::CancelDelete,
        _ => Action::None,
    }
}

fn handle_form_key(form: &mut FormState, key: KeyEvent, profiles: &[Profile]) -> Action {
    match key.code {
        KeyCode::Esc => Action::CancelForm,
        KeyCode::Enter => Action::SubmitForm,
        KeyCode::Tab | KeyCode::Down => {
            form.move_focus(1);
            Action::None
        }
        KeyCode::BackTab | KeyCode::Up => {
            form.move_focus(-1);
            Action::None
        }
        KeyCode::Left => {
            match form.focused_field() {
                FormField::Direction => form.cycle_direction(false),
                FormField::Profile => form.cycle_profile(false, profiles),
                _ => {}
            }
            Action::None
        }
        KeyCode::Right => {
            match form.focused_field() {
                FormField::Direction => form.cycle_direction(true),
                FormField::Profile => form.cycle_profile(true, profiles),
                _ => {}
            }
            Action::None
        }
        KeyCode::Backspace => {
            if let Some(text) = form.active_text_mut() {
                text.pop();
            }
            Action::None
        }
        KeyCode::Char(c) => {
            if let Some(text) = form.active_text_mut() {
                text.push(c);
            }
            Action::None
        }
        _ => Action::None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormField {
    Name,
    Direction,
    Profile,
    LocalBindAddr,
    LocalBindPort,
    RemoteHost,
    RemotePort,
}

pub struct FormState {
    pub editing_id: Option<Uuid>,
    pub focus_index: usize,
    pub name: String,
    pub direction: Direction,
    /// Name of the selected `Profile` (host/port/username/auth/jump) this
    /// tunnel connects through. Profiles themselves aren't editable here
    /// yet — add one under `[[profiles]]` in config.toml first, then pick
    /// it with left/right on this field.
    pub profile_name: String,
    pub local_bind_addr: String,
    pub local_bind_port: String,
    pub remote_host: String,
    pub remote_port: String,
    pub error: Option<String>,
}

impl FormState {
    pub fn new_add(profiles: &[Profile]) -> Self {
        Self {
            editing_id: None,
            focus_index: 0,
            name: String::new(),
            direction: Direction::Local,
            profile_name: profiles.first().map(|p| p.name.clone()).unwrap_or_default(),
            local_bind_addr: "127.0.0.1".to_string(),
            local_bind_port: String::new(),
            remote_host: String::new(),
            remote_port: String::new(),
            error: None,
        }
    }

    pub fn from_definition(def: &TunnelDefinition) -> Self {
        Self {
            editing_id: Some(def.id),
            focus_index: 0,
            name: def.name.clone(),
            direction: def.direction,
            profile_name: def.profile.clone(),
            local_bind_addr: def.local_bind.bind_addr.clone(),
            local_bind_port: def.local_bind.port.to_string(),
            remote_host: def
                .remote
                .as_ref()
                .map(|r| r.host.clone())
                .unwrap_or_default(),
            remote_port: def
                .remote
                .as_ref()
                .map(|r| r.port.to_string())
                .unwrap_or_default(),
            error: None,
        }
    }

    pub fn visible_fields(&self) -> Vec<FormField> {
        let mut fields = vec![
            FormField::Name,
            FormField::Direction,
            FormField::Profile,
            FormField::LocalBindAddr,
            FormField::LocalBindPort,
        ];
        if self.direction != Direction::Dynamic {
            fields.push(FormField::RemoteHost);
            fields.push(FormField::RemotePort);
        }
        fields
    }

    pub fn focused_field(&self) -> FormField {
        let fields = self.visible_fields();
        fields[self.focus_index.min(fields.len().saturating_sub(1))]
    }

    pub fn move_focus(&mut self, delta: isize) {
        let len = self.visible_fields().len() as isize;
        let mut idx = self.focus_index as isize + delta;
        if idx < 0 {
            idx = len - 1;
        }
        if idx >= len {
            idx = 0;
        }
        self.focus_index = idx as usize;
    }

    pub fn cycle_direction(&mut self, forward: bool) {
        self.direction = match (self.direction, forward) {
            (Direction::Local, true) => Direction::Remote,
            (Direction::Remote, true) => Direction::Dynamic,
            (Direction::Dynamic, true) => Direction::Local,
            (Direction::Local, false) => Direction::Dynamic,
            (Direction::Remote, false) => Direction::Local,
            (Direction::Dynamic, false) => Direction::Remote,
        };
        self.clamp_focus();
    }

    pub fn cycle_profile(&mut self, forward: bool, profiles: &[Profile]) {
        if profiles.is_empty() {
            return;
        }
        let current = profiles.iter().position(|p| p.name == self.profile_name);
        let next = match (current, forward) {
            (Some(i), true) => (i + 1) % profiles.len(),
            (Some(0), false) => profiles.len() - 1,
            (Some(i), false) => i - 1,
            (None, _) => 0,
        };
        self.profile_name = profiles[next].name.clone();
    }

    fn clamp_focus(&mut self) {
        self.focus_index = self
            .focus_index
            .min(self.visible_fields().len().saturating_sub(1));
    }

    pub fn active_text_mut(&mut self) -> Option<&mut String> {
        match self.focused_field() {
            FormField::Name => Some(&mut self.name),
            FormField::LocalBindAddr => Some(&mut self.local_bind_addr),
            FormField::LocalBindPort => Some(&mut self.local_bind_port),
            FormField::RemoteHost => Some(&mut self.remote_host),
            FormField::RemotePort => Some(&mut self.remote_port),
            FormField::Direction | FormField::Profile => None,
        }
    }

    pub fn build_definition(&self) -> Result<TunnelDefinition, String> {
        if self.name.trim().is_empty() {
            return Err("Name is required".to_string());
        }
        if self.profile_name.trim().is_empty() {
            return Err(
                "No profile selected — add one under [[profiles]] in config.toml first".to_string(),
            );
        }

        let local_bind_port: u16 = self
            .local_bind_port
            .trim()
            .parse()
            .map_err(|_| "Local port must be a number".to_string())?;

        let remote = if self.direction == Direction::Dynamic {
            None
        } else {
            if self.remote_host.trim().is_empty() {
                return Err("Remote host is required".to_string());
            }
            let remote_port: u16 = self
                .remote_port
                .trim()
                .parse()
                .map_err(|_| "Remote port must be a number".to_string())?;
            Some(HostPort {
                host: self.remote_host.trim().to_string(),
                port: remote_port,
            })
        };

        Ok(TunnelDefinition {
            id: self.editing_id.unwrap_or_else(Uuid::new_v4),
            name: self.name.trim().to_string(),
            direction: self.direction,
            profile: self.profile_name.clone(),
            local_bind: SocketAddrSpec {
                bind_addr: self.local_bind_addr.trim().to_string(),
                port: local_bind_port,
            },
            remote,
            autostart: false,
            enabled: true,
        })
    }
}
