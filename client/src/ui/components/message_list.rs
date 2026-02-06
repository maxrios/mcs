use std::collections::VecDeque;

use chrono::{DateTime, Local, TimeZone, Utc};
use protocol::ChatPacket;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};

#[allow(clippy::cast_possible_truncation)]
pub fn draw(
    f: &mut Frame,
    area: Rect,
    messages: &VecDeque<ChatPacket>,
    current_user: &str,
    scroll_offset: &mut u16,
    should_request_history: &mut bool,
    history_request_timestamp: &mut Option<i64>,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Chat History ");
    let inner_width = area.width.saturating_sub(2) as usize;
    let inner_height = area.height.saturating_sub(2);
    let mut total_visual_lines = 0;

    let lines: Vec<Line> = messages
        .iter()
        .map(|msg| {
            let time_str = format_timestamp(msg.timestamp);
            let line = if msg.sender == "server" {
                Line::from(Span::styled(
                    format!("[{}] {}", time_str, msg.content),
                    Style::default().fg(Color::DarkGray),
                ))
            } else {
                let color = if msg.sender == current_user {
                    Color::Green
                } else {
                    Color::Blue
                };

                Line::from(vec![
                    Span::styled(
                        format!("[{}] {}: ", time_str, msg.sender),
                        Style::default().fg(color),
                    ),
                    Span::raw(&msg.content),
                ])
            };
            let line_len = line.iter().len();
            let lines_taken = if inner_width > 0 {
                line_len.div_ceil(inner_width) as u16
            } else {
                1
            };

            let lines_taken = lines_taken.max(1);
            total_visual_lines += lines_taken;

            line
        })
        .collect();

    let max_scroll = total_visual_lines.saturating_sub(inner_height);
    *scroll_offset = (*scroll_offset).min(max_scroll);
    *should_request_history = max_scroll == *scroll_offset;
    if *should_request_history && let Some(packet) = messages.iter().peekable().peek() {
        *history_request_timestamp = Some(packet.timestamp);
    }
    let scroll_from_top = max_scroll.saturating_sub(*scroll_offset);

    let paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll_from_top, 0));

    f.render_widget(paragraph, area);
}

fn format_timestamp(ts: i64) -> String {
    let dt: DateTime<Utc> = Utc.timestamp_opt(ts, 0).earliest().unwrap_or_else(Utc::now);
    let local: DateTime<Local> = DateTime::from(dt);
    local.format("%Y-%m-%d %H:%M").to_string()
}
