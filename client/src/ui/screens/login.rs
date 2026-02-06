use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::{
    app::{App, LoginStep},
    ui::{centered_rect, components::input},
};

#[allow(clippy::cast_possible_truncation)]
pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let bg_block = Block::default()
        .borders(Borders::NONE)
        .style(Style::default().bg(Color::Black));
    f.render_widget(bg_block, area);

    let popup_area = centered_rect(area, 60, 40);
    f.render_widget(Clear, popup_area);

    let popup_block = Block::default()
        .borders(Borders::ALL)
        .title(" Connect to Server ")
        .style(Style::default().bg(Color::DarkGray));

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
        ])
        .split(popup_block.inner(popup_area));
    f.render_widget(popup_block, area);

    let ip_content = if app.login.step == LoginStep::Ip {
        &app.ui.input_buffer
    } else {
        &app.login.ip
    };
    input::draw(
        f,
        layout[0],
        "Server IP",
        ip_content,
        app.login.step == LoginStep::Ip,
    );

    let user_content = if app.login.step == LoginStep::Username {
        &app.ui.input_buffer
    } else {
        &app.login.user
    };
    input::draw(
        f,
        layout[1],
        "Username",
        user_content,
        app.login.step == LoginStep::Username,
    );

    let pass_display = if app.login.step == LoginStep::Password {
        app.ui.input_buffer.chars().map(|_| '*').collect::<String>()
    } else {
        String::new()
    };

    input::draw(
        f,
        layout[2],
        "Password",
        &pass_display,
        app.login.step == LoginStep::Password,
    );

    let bottom_paragraph = &app.ui.error_message.as_mut().map_or_else(
        || {
            Paragraph::new("Tab/Enter to next • Esc to quit")
                .style(Style::default().fg(Color::Gray))
        },
        |err| {
            Paragraph::new(format!("Error: {err}"))
                .style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
        },
    );

    f.render_widget(bottom_paragraph, layout[3]);

    let active_chunk = match app.login.step {
        LoginStep::Ip => layout[0],
        LoginStep::Username => layout[1],
        LoginStep::Password => layout[2],
    };
    f.set_cursor_position((
        active_chunk.x + 1 + app.ui.input_buffer.len() as u16,
        active_chunk.y + 1,
    ));
}
