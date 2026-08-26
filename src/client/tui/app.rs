use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::TableState;
use uuid::Uuid;

use crate::config::schema::{
    AuthMethod, Direction, HostPort, Profile, SocketAddrSpec, TunnelDefinition,
};
use crate::ipc::protocol::TunnelSnapshot;
use crate::model::TunnelEvent;

pub struct App {
    pub should_quit: bool,
    pub daemon_connected: bool,
    pub tunnels: Vec<TunnelSnapshot>,
    pub profiles: Vec<Profile>,
    pub table_state: TableState,
    pub profile_table_state: TableState,
    pub panel: ListPanel,
    pub mode: Mode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListPanel {
    Tunnels,
    Profiles,
}

pub enum Mode {
    List,
    Form(Box<FormState>),
    ProfileForm(Box<ProfileFormState>),
    ConfirmDelete(Uuid),
    ConfirmDeleteProfile { name: String, error: Option<String> },
}

/// What a keypress should trigger; the actual daemon call happens in the
/// async main loop, which is why App itself stays synchronous. Add/Edit/
/// Delete/Submit/Confirm are shared between the Tunnels and Profiles
/// panels — the main loop decides which concrete request to send based on
/// `app.panel`.
pub enum Action {
    None,
    Quit,
    TogglePanel,
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
            profile_table_state: TableState::default(),
            panel: ListPanel::Tunnels,
            mode: Mode::List,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        match &mut self.mode {
            Mode::List => self.handle_list_key(key),
            Mode::Form(form) => handle_form_key(form, key, &self.profiles),
            Mode::ProfileForm(form) => handle_profile_form_key(form, key, &self.profiles),
            Mode::ConfirmDelete(_) => handle_confirm_key(key),
            Mode::ConfirmDeleteProfile { .. } => handle_confirm_key(key),
        }
    }

    fn handle_list_key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char('q') => Action::Quit,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,
            KeyCode::Char('p') => Action::TogglePanel,
            KeyCode::Down | KeyCode::Char('j') => {
                match self.panel {
                    ListPanel::Tunnels => self.select_next(),
                    ListPanel::Profiles => self.select_next_profile(),
                }
                Action::None
            }
            KeyCode::Up | KeyCode::Char('k') => {
                match self.panel {
                    ListPanel::Tunnels => self.select_previous(),
                    ListPanel::Profiles => self.select_previous_profile(),
                }
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

    pub fn select_next_profile(&mut self) {
        if self.profiles.is_empty() {
            return;
        }
        let next = match self.profile_table_state.selected() {
            Some(i) if i + 1 < self.profiles.len() => i + 1,
            _ => 0,
        };
        self.profile_table_state.select(Some(next));
    }

    pub fn select_previous_profile(&mut self) {
        if self.profiles.is_empty() {
            return;
        }
        let prev = match self.profile_table_state.selected() {
            Some(0) | None => self.profiles.len() - 1,
            Some(i) => i - 1,
        };
        self.profile_table_state.select(Some(prev));
    }

    pub fn selected_snapshot(&self) -> Option<&TunnelSnapshot> {
        self.table_state
            .selected()
            .and_then(|i| self.tunnels.get(i))
    }

    pub fn selected_profile(&self) -> Option<&Profile> {
        self.profile_table_state
            .selected()
            .and_then(|i| self.profiles.get(i))
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
    /// tunnel connects through. Pick one with left/right on this field, or
    /// switch to the Profiles panel (`p`) to add one first.
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
            return Err("No profile selected — add one in the Profiles panel first".to_string());
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

fn handle_profile_form_key(
    form: &mut ProfileFormState,
    key: KeyEvent,
    profiles: &[Profile],
) -> Action {
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
                ProfileFormField::AuthKind => form.cycle_auth_kind(false),
                ProfileFormField::Jump => form.cycle_jump(false, profiles),
                _ => {}
            }
            Action::None
        }
        KeyCode::Right => {
            match form.focused_field() {
                ProfileFormField::AuthKind => form.cycle_auth_kind(true),
                ProfileFormField::Jump => form.cycle_jump(true, profiles),
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
pub enum ProfileFormField {
    Name,
    Host,
    Port,
    Username,
    AuthKind,
    AuthPassword,
    AuthKeyPath,
    AuthKeyPassphrase,
    Jump,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthKind {
    Password,
    PrivateKey,
    Agent,
}

pub struct ProfileFormState {
    /// `Some(name)` when editing an existing profile — the name is that
    /// profile's identity and can't be changed this way (delete and
    /// recreate to rename); `None` while adding, when `name` is a normal
    /// editable text field.
    pub editing_name: Option<String>,
    pub focus_index: usize,
    pub name: String,
    pub host: String,
    pub port: String,
    pub username: String,
    pub auth_kind: AuthKind,
    pub password: String,
    pub key_path: String,
    pub key_passphrase: String,
    pub jump_name: Option<String>,
    pub error: Option<String>,
}

impl ProfileFormState {
    pub fn new_add(_profiles: &[Profile]) -> Self {
        Self {
            editing_name: None,
            focus_index: 0,
            name: String::new(),
            host: String::new(),
            port: "22".to_string(),
            username: String::new(),
            auth_kind: AuthKind::Agent,
            password: String::new(),
            key_path: String::new(),
            key_passphrase: String::new(),
            jump_name: None,
            error: None,
        }
    }

    pub fn from_profile(profile: &Profile, _profiles: &[Profile]) -> Self {
        let (auth_kind, password, key_path, key_passphrase) = match &profile.auth {
            AuthMethod::Password { password } => (
                AuthKind::Password,
                password.clone().unwrap_or_default(),
                String::new(),
                String::new(),
            ),
            AuthMethod::PrivateKey { path, passphrase } => (
                AuthKind::PrivateKey,
                String::new(),
                path.to_string_lossy().into_owned(),
                passphrase.clone().unwrap_or_default(),
            ),
            AuthMethod::Agent => (AuthKind::Agent, String::new(), String::new(), String::new()),
        };
        Self {
            editing_name: Some(profile.name.clone()),
            focus_index: 0,
            name: profile.name.clone(),
            host: profile.host.clone(),
            port: profile.port.to_string(),
            username: profile.username.clone(),
            auth_kind,
            password,
            key_path,
            key_passphrase,
            jump_name: profile.jump.clone(),
            error: None,
        }
    }

    pub fn visible_fields(&self) -> Vec<ProfileFormField> {
        let mut fields = Vec::new();
        if self.editing_name.is_none() {
            fields.push(ProfileFormField::Name);
        }
        fields.push(ProfileFormField::Host);
        fields.push(ProfileFormField::Port);
        fields.push(ProfileFormField::Username);
        fields.push(ProfileFormField::AuthKind);
        match self.auth_kind {
            AuthKind::Password => fields.push(ProfileFormField::AuthPassword),
            AuthKind::PrivateKey => {
                fields.push(ProfileFormField::AuthKeyPath);
                fields.push(ProfileFormField::AuthKeyPassphrase);
            }
            AuthKind::Agent => {}
        }
        fields.push(ProfileFormField::Jump);
        fields
    }

    pub fn focused_field(&self) -> ProfileFormField {
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

    fn clamp_focus(&mut self) {
        self.focus_index = self
            .focus_index
            .min(self.visible_fields().len().saturating_sub(1));
    }

    pub fn cycle_auth_kind(&mut self, forward: bool) {
        self.auth_kind = match (self.auth_kind, forward) {
            (AuthKind::Password, true) => AuthKind::PrivateKey,
            (AuthKind::PrivateKey, true) => AuthKind::Agent,
            (AuthKind::Agent, true) => AuthKind::Password,
            (AuthKind::Password, false) => AuthKind::Agent,
            (AuthKind::PrivateKey, false) => AuthKind::Password,
            (AuthKind::Agent, false) => AuthKind::PrivateKey,
        };
        self.clamp_focus();
    }

    /// Candidates exclude this profile itself and — matching the current
    /// single-hop constraint enforced in `resolve::resolve_connection` —
    /// any profile that already has a jump of its own.
    pub fn cycle_jump(&mut self, forward: bool, profiles: &[Profile]) {
        let candidates: Vec<Option<String>> = std::iter::once(None)
            .chain(
                profiles
                    .iter()
                    .filter(|p| p.name != self.name && p.jump.is_none())
                    .map(|p| Some(p.name.clone())),
            )
            .collect();
        if candidates.is_empty() {
            return;
        }
        let current = candidates
            .iter()
            .position(|c| c == &self.jump_name)
            .unwrap_or(0);
        let next = if forward {
            (current + 1) % candidates.len()
        } else {
            (current + candidates.len() - 1) % candidates.len()
        };
        self.jump_name = candidates[next].clone();
    }

    pub fn active_text_mut(&mut self) -> Option<&mut String> {
        match self.focused_field() {
            ProfileFormField::Name => Some(&mut self.name),
            ProfileFormField::Host => Some(&mut self.host),
            ProfileFormField::Port => Some(&mut self.port),
            ProfileFormField::Username => Some(&mut self.username),
            ProfileFormField::AuthPassword => Some(&mut self.password),
            ProfileFormField::AuthKeyPath => Some(&mut self.key_path),
            ProfileFormField::AuthKeyPassphrase => Some(&mut self.key_passphrase),
            ProfileFormField::AuthKind | ProfileFormField::Jump => None,
        }
    }

    pub fn build_profile(&self) -> Result<Profile, String> {
        let name = if let Some(existing) = &self.editing_name {
            existing.clone()
        } else {
            if self.name.trim().is_empty() {
                return Err("Name is required".to_string());
            }
            self.name.trim().to_string()
        };
        if self.host.trim().is_empty() {
            return Err("Host is required".to_string());
        }
        let port: u16 = self
            .port
            .trim()
            .parse()
            .map_err(|_| "Port must be a number".to_string())?;
        if self.username.trim().is_empty() {
            return Err("Username is required".to_string());
        }
        let auth = match self.auth_kind {
            AuthKind::Password => AuthMethod::Password {
                password: if self.password.is_empty() {
                    None
                } else {
                    Some(self.password.clone())
                },
            },
            AuthKind::PrivateKey => {
                if self.key_path.trim().is_empty() {
                    return Err("Key path is required".to_string());
                }
                AuthMethod::PrivateKey {
                    path: PathBuf::from(self.key_path.trim()),
                    passphrase: if self.key_passphrase.is_empty() {
                        None
                    } else {
                        Some(self.key_passphrase.clone())
                    },
                }
            }
            AuthKind::Agent => AuthMethod::Agent,
        };
        Ok(Profile {
            name,
            host: self.host.trim().to_string(),
            port,
            username: self.username.trim().to_string(),
            auth,
            jump: self.jump_name.clone(),
        })
    }
}
