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

pub fn draw(f: &mut Frame, area: Rect, messages: &VecDeque<ChatPacket>, current_user: &str) {
    let lines: Vec<Line> = messages
        .iter()
        .map(|msg| {
            let time_str = format_timestamp(msg.timestamp);
            if msg.sender == "server" {
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
            }
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Chat History ");

    let paragraph = Paragraph::new(Text::from(lines))
        .block(block)
        .wrap(Wrap { trim: false });

    f.render_widget(paragraph, area);
}

fn format_timestamp(ts: i64) -> String {
    let dt: DateTime<Utc> = Utc.timestamp_opt(ts, 0).earliest().unwrap_or_else(Utc::now);
    let local: DateTime<Local> = DateTime::from(dt);
    local.format("%Y-%m-%d %H:%M").to_string()
}
