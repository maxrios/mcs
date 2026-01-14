use chrono::{DateTime, Local, TimeZone, Utc};
use protocol::ChatPacket;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use tokio::sync::mpsc;

pub struct ChatApp {
    pub username: String,
    pub input: String,
    pub messages: Vec<ChatPacket>,
    pub network_tx: mpsc::UnboundedSender<ChatPacket>,
    pub scroll: u16,
    pub scroll_limit: u16,
}

impl ChatApp {
    pub fn new(username: String, network_tx: mpsc::UnboundedSender<ChatPacket>) -> Self {
        Self {
            username,
            input: String::new(),
            messages: Vec::new(),
            network_tx,
            scroll: 0u16,
            scroll_limit: 0u16,
        }
    }

    pub fn submit_message(&mut self) {
        if !self.input.trim().is_empty() {
            let _ = self.network_tx.send(ChatPacket::new_user_packet(
                self.username.clone(),
                self.input.clone(),
            ));
            self.input.clear();
        }
    }

    pub fn update_ui(&mut self, f: &mut Frame) {
        let input_height = self.get_input_height(f);
        let frame_width = f.area().width.saturating_sub(2) as usize;

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(input_height)])
            .split(f.area());

        let mut total_lines = 0u16;
        let mut message_lines = Vec::new();
        for m in self.messages.iter() {
            let color = if m.sender == "server" {
                Color::DarkGray
            } else {
                Color::White
            };

            let converted_time = convert_timestamp_to_local_date(m.timestamp);
            let unformatted_time = match converted_time {
                Some(time) => time,
                _ => continue,
            };
            let formatted_time = unformatted_time.format("%D %l:%m %p");

            let text = if m.sender == "server" {
                format!("[{}] {}", formatted_time, m.content)
            } else {
                format!("[{}] {}: {}", formatted_time, m.sender, m.content)
            };

            message_lines.push(Line::from(Span::styled(
                text.clone(),
                Style::default().fg(color),
            )));

            if frame_width > 0 {
                let m_len = text.len();
                let lines_for_this_msg = ((m_len.saturating_sub(1) / frame_width) + 1) as u16;
                total_lines += lines_for_this_msg;
            } else {
                total_lines += 1;
            }
        }

        self.scroll_limit = total_lines;

        let history_viewport_height = chunks[0].height.saturating_sub(2);
        let max_scroll = total_lines.saturating_sub(history_viewport_height);
        if self.scroll > max_scroll {
            self.scroll = max_scroll;
        }

        let history_text = Text::from(message_lines);
        let chat_history = Paragraph::new(history_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" Chat History - {} ", self.username))
                    .border_style(Style::default().fg(Color::Cyan)),
            )
            .wrap(Wrap { trim: false })
            .scroll((self.scroll, 0));

        f.render_widget(chat_history, chunks[0]);

        let input = Paragraph::new(self.input.as_str())
            .style(Style::default().fg(Color::Yellow))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Message (Esc to quit) ")
                    .border_style(Style::default().fg(Color::Green)),
            )
            .wrap(Wrap { trim: false });

        f.render_widget(input, chunks[1]);

        let (cursor_x, cursor_y) = self.get_cursor_position(f);
        f.set_cursor_position((chunks[1].x + cursor_x + 1, chunks[1].y + cursor_y + 1));
    }

    fn get_input_height(&self, f: &Frame) -> u16 {
        let area = f.area();

        let text_width = area.width.saturating_sub(2) as usize;

        let input_lines = if text_width > 0 {
            ((self.input.len() / text_width) as u16) + 1
        } else {
            1
        };

        input_lines + 2
    }

    fn get_cursor_position(&self, f: &Frame) -> (u16, u16) {
        let area = f.area();

        let text_width = area.width.saturating_sub(2) as usize;

        // x = (total chars % width) + 1 (for border)
        // y = (total chars / width) + 1 (for border)
        let cursor_x = if text_width > 0 {
            (self.input.len() % text_width) as u16
        } else {
            0
        };
        let cursor_y = if text_width > 0 {
            (self.input.len() / text_width) as u16
        } else {
            0
        };

        (cursor_x, cursor_y)
    }
}

fn convert_timestamp_to_local_date(timestamp: i64) -> Option<DateTime<Local>> {
    let utc_datetime = Utc.timestamp_opt(timestamp, 0).single()?;
    let local_datetime = utc_datetime.with_timezone(&Local);

    Some(local_datetime)
}
