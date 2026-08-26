use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table, TableState},
};

use crate::config::schema::{AuthMethod, Direction, Profile};
use crate::ipc::protocol::TunnelSnapshot;
use crate::model::TunnelState;

pub fn draw(
    frame: &mut Frame,
    area: Rect,
    tunnels: &[TunnelSnapshot],
    table_state: &mut TableState,
) {
    let rows = tunnels.iter().map(|snapshot| {
        let direction = match snapshot.def.direction {
            Direction::Local => "L",
            Direction::Remote => "R",
            Direction::Dynamic => "D",
        };
        let remote = snapshot
            .def
            .remote
            .as_ref()
            .map(|r| format!("{}:{}", r.host, r.port))
            .unwrap_or_else(|| "-".to_string());
        let local_remote = format!(
            "{}:{} <-> {remote}",
            snapshot.def.local_bind.bind_addr, snapshot.def.local_bind.port
        );
        let (status_text, status_color) = status_style(&snapshot.state);

        Row::new(vec![
            Cell::from(snapshot.def.name.clone()),
            Cell::from(direction),
            Cell::from(local_remote),
            Cell::from(status_text).style(Style::new().fg(status_color)),
        ])
    });

    let widths = [
        Constraint::Percentage(25),
        Constraint::Length(4),
        Constraint::Percentage(45),
        Constraint::Percentage(20),
    ];

    let table = Table::new(rows, widths)
        .header(Row::new(vec!["Name", "Type", "Local <-> Remote", "Status"]))
        .block(Block::default().borders(Borders::ALL).title("Tunnels"))
        .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED));

    frame.render_stateful_widget(table, area, table_state);
}

pub fn draw_profiles(
    frame: &mut Frame,
    area: Rect,
    profiles: &[Profile],
    table_state: &mut TableState,
) {
    let rows = profiles.iter().map(|profile| {
        let auth = match &profile.auth {
            AuthMethod::Password { .. } => "password",
            AuthMethod::PrivateKey { .. } => "private key",
            AuthMethod::Agent => "agent",
        };
        Row::new(vec![
            Cell::from(profile.name.clone()),
            Cell::from(format!("{}:{}", profile.host, profile.port)),
            Cell::from(profile.username.clone()),
            Cell::from(auth),
            Cell::from(profile.jump.clone().unwrap_or_else(|| "-".to_string())),
        ])
    });

    let widths = [
        Constraint::Percentage(20),
        Constraint::Percentage(30),
        Constraint::Percentage(15),
        Constraint::Percentage(15),
        Constraint::Percentage(20),
    ];

    let table = Table::new(rows, widths)
        .header(Row::new(vec![
            "Name",
            "Host:Port",
            "Username",
            "Auth",
            "Jump",
        ]))
        .block(Block::default().borders(Borders::ALL).title("Profiles"))
        .row_highlight_style(Style::new().add_modifier(Modifier::REVERSED));

    frame.render_stateful_widget(table, area, table_state);
}

fn status_style(state: &TunnelState) -> (String, Color) {
    match state {
        TunnelState::Stopped => ("stopped".to_string(), Color::Gray),
        TunnelState::Connecting => ("connecting".to_string(), Color::Yellow),
        TunnelState::Connected { .. } => ("connected".to_string(), Color::Green),
        TunnelState::Error(err) => (format!("error: {err}"), Color::Red),
        TunnelState::Stopping => ("stopping".to_string(), Color::Yellow),
    }
}
