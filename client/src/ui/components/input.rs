use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph, Wrap},
};

/// Renders a bordered input box.
///
/// * `title` - The text displayed in the top border.
/// * `content` - The text inside the box.
/// * `is_focused` - If true, the border turns yellow; otherwise, white.
pub fn draw(f: &mut Frame, area: Rect, title: &str, content: &str, is_focused: bool) {
    let border_style = if is_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::White)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {title} "))
        .border_style(border_style);
    let p = Paragraph::new(content)
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(p, area);
}
