mod form_view;
mod list_view;
mod profile_form_view;

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::Style,
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::app::{App, ListPanel, Mode};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);

    match app.panel {
        ListPanel::Tunnels => list_view::draw(frame, chunks[0], &app.tunnels, &mut app.table_state),
        ListPanel::Profiles => list_view::draw_profiles(
            frame,
            chunks[0],
            &app.profiles,
            &mut app.profile_table_state,
        ),
    }

    let help_text = match &app.mode {
        Mode::List => {
            "q: quit  p: switch panel  a: add  e: edit  d: delete  enter/s: start-stop  r: reload"
        }
        Mode::Form(_) | Mode::ProfileForm(_) => {
            "tab: next field  left/right: change  enter: save  esc: cancel"
        }
        Mode::ConfirmDelete(_) => "y: confirm delete  n/esc: cancel",
        Mode::ConfirmDeleteProfile { .. } => "y: confirm delete  n/esc: cancel",
    };
    let status = if app.daemon_connected {
        "connected"
    } else {
        "disconnected"
    };
    let help = Paragraph::new(Line::from(format!("{help_text}  |  daemon: {status}")))
        .style(Style::new().dim());
    frame.render_widget(help, chunks[1]);

    match &app.mode {
        Mode::Form(form) => {
            let popup = centered_rect(70, 70, area);
            frame.render_widget(Clear, popup);
            form_view::draw(frame, popup, form);
        }
        Mode::ProfileForm(form) => {
            let popup = centered_rect(70, 70, area);
            frame.render_widget(Clear, popup);
            profile_form_view::draw(frame, popup, form);
        }
        Mode::ConfirmDelete(_) => {
            let popup = centered_rect(50, 20, area);
            frame.render_widget(Clear, popup);
            let block = Block::default()
                .borders(Borders::ALL)
                .title("Confirm Delete");
            let text = Paragraph::new("Delete this tunnel? (y/n)").block(block);
            frame.render_widget(text, popup);
        }
        Mode::ConfirmDeleteProfile { error, .. } => {
            let popup = centered_rect(50, 20, area);
            frame.render_widget(Clear, popup);
            let block = Block::default()
                .borders(Borders::ALL)
                .title("Confirm Delete");
            let body = match error {
                Some(err) => format!("Delete this profile? (y/n)\n\nError: {err}"),
                None => "Delete this profile? (y/n)".to_string(),
            };
            let text = Paragraph::new(body).block(block);
            frame.render_widget(text, popup);
        }
        Mode::List => {}
    }
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);
    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}
