use ratatui::{
    Frame,
    layout::{Constraint, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Row, Table},
};

use crate::config::schema::Direction;
use crate::ipc::protocol::TunnelSnapshot;
use crate::model::TunnelState;

pub fn draw(frame: &mut Frame, area: Rect, tunnels: &[TunnelSnapshot]) {
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
        .block(Block::default().borders(Borders::ALL).title("Tunnels"));

    frame.render_widget(table, area);
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
