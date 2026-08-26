use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

use crate::client::tui::app::{AuthKind, ProfileFormField, ProfileFormState};

pub fn draw(frame: &mut Frame, area: Rect, form: &ProfileFormState) {
    let title = match &form.editing_name {
        Some(name) => format!("Edit Profile: {name}"),
        None => "Add Profile".to_string(),
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let fields = form.visible_fields();
    let mut constraints: Vec<Constraint> = fields.iter().map(|_| Constraint::Length(1)).collect();
    constraints.push(Constraint::Min(1));
    let rows = Layout::vertical(constraints).split(inner);

    for (i, field) in fields.iter().enumerate() {
        let focused = i == form.focus_index;
        let (label, value) = field_label_value(form, *field);
        let style = if focused {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let line = Paragraph::new(Line::from(format!("{label:<16}{value}"))).style(style);
        frame.render_widget(line, rows[i]);
    }

    if let Some(error) = &form.error {
        let error_line = Paragraph::new(Line::from(format!("Error: {error}")))
            .style(Style::default().fg(Color::Red));
        frame.render_widget(error_line, rows[fields.len()]);
    }
}

fn field_label_value(form: &ProfileFormState, field: ProfileFormField) -> (&'static str, String) {
    match field {
        ProfileFormField::Name => ("Name", form.name.clone()),
        ProfileFormField::Host => ("Host", form.host.clone()),
        ProfileFormField::Port => ("Port", form.port.clone()),
        ProfileFormField::Username => ("Username", form.username.clone()),
        ProfileFormField::AuthKind => ("Auth", auth_kind_label(form.auth_kind).to_string()),
        ProfileFormField::AuthPassword => ("Password", "*".repeat(form.password.len())),
        ProfileFormField::AuthKeyPath => ("Key Path", form.key_path.clone()),
        ProfileFormField::AuthKeyPassphrase => {
            ("Key Passphrase", "*".repeat(form.key_passphrase.len()))
        }
        ProfileFormField::Jump => (
            "Jump",
            form.jump_name
                .clone()
                .unwrap_or_else(|| "(none)".to_string()),
        ),
    }
}

fn auth_kind_label(kind: AuthKind) -> &'static str {
    match kind {
        AuthKind::Password => "Password",
        AuthKind::PrivateKey => "Private Key",
        AuthKind::Agent => "Agent",
    }
}
