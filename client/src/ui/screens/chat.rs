use ratatui::{
    Frame,
    layout::{Constraint, Layout},
};

use crate::{
    app::App,
    ui::components::{input, message_list},
};

#[allow(clippy::cast_possible_truncation)]
pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let chunks = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(3)])
        .split(area);

    message_list::draw(
        f,
        chunks[0],
        &app.chat.messages,
        &app.chat.username,
        &mut app.chat.scroll_offset,
        &mut app.chat.should_request_history,
        &mut app.chat.history_request_timestamp,
    );

    input::draw(
        f,
        chunks[1],
        "Message (Esc to quit)",
        &app.ui.input_buffer,
        true,
    );
    f.set_cursor_position((
        chunks[1].x + 1 + app.ui.input_buffer.len() as u16,
        chunks[1].y + 1,
    ));
}
