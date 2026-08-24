use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

use crate::client::tui::app::{FormField, FormState};
use crate::config::schema::Direction;

pub fn draw(frame: &mut Frame, area: Rect, form: &FormState) {
    let title = if form.editing_id.is_some() {
        "Edit Tunnel"
    } else {
        "Add Tunnel"
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

fn field_label_value(form: &FormState, field: FormField) -> (&'static str, String) {
    match field {
        FormField::Name => ("Name", form.name.clone()),
        FormField::Direction => ("Direction", direction_label(form.direction).to_string()),
        FormField::Profile => (
            "Profile",
            if form.profile_name.is_empty() {
                "(none — add one in config.toml)".to_string()
            } else {
                form.profile_name.clone()
            },
        ),
        FormField::LocalBindAddr => ("Local Bind Addr", form.local_bind_addr.clone()),
        FormField::LocalBindPort => ("Local Bind Port", form.local_bind_port.clone()),
        FormField::RemoteHost => ("Remote Host", form.remote_host.clone()),
        FormField::RemotePort => ("Remote Port", form.remote_port.clone()),
    }
}

fn direction_label(direction: Direction) -> &'static str {
    match direction {
        Direction::Local => "Local (-L)",
        Direction::Remote => "Remote (-R)",
        Direction::Dynamic => "Dynamic/SOCKS (-D)",
    }
}
