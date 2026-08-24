mod list_view;

use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::Style,
    text::Line,
    widgets::Paragraph,
};

use super::app::App;

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);

    list_view::draw(frame, chunks[0], &app.tunnels);

    let status = if app.daemon_connected {
        "connected"
    } else {
        "disconnected"
    };
    let help = Paragraph::new(Line::from(format!("q: quit  |  daemon: {status}")))
        .style(Style::new().dim());
    frame.render_widget(help, chunks[1]);
}
