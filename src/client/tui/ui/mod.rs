use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::Style,
    text::Line,
    widgets::{Block, Borders, Paragraph},
};

use super::app::App;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);

    let status = if app.daemon_connected {
        "connected"
    } else {
        "disconnected"
    };
    let body = Paragraph::new(Line::from(format!("pmanager — daemon: {status}")))
        .block(Block::default().borders(Borders::ALL).title("Port Manager"));
    frame.render_widget(body, chunks[0]);

    let help = Paragraph::new("q: quit").style(Style::new().dim());
    frame.render_widget(help, chunks[1]);
}
