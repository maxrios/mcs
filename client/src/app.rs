use protocol::ChatPacket;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use tokio::sync::mpsc;

use crate::state::ChatEvent;

#[derive(PartialEq, Eq)]
pub enum AppState {
    Login,
    Chat,
}

pub struct ChatApp {
    pub state: AppState,
    pub username: String,
    pub input: String,
    pub messages: Vec<ChatEvent>,
    pub network_tx: mpsc::UnboundedSender<ChatPacket>,
    pub scroll: u16,
    pub scroll_limit: u16,

    pub login_ip: String,
    pub login_user: String,
    pub login_field_idx: usize, // 0 = IP, 1 = Username
    pub connection_error: Option<String>,
}

impl ChatApp {
    pub const fn new(network_tx: mpsc::UnboundedSender<ChatPacket>) -> Self {
        Self {
            state: AppState::Login,
            username: String::new(),
            input: String::new(),
            messages: Vec::new(),
            network_tx,
            scroll: 0u16,
            scroll_limit: 0u16,
            login_ip: String::new(),
            login_user: String::new(),
            login_field_idx: 0,
            connection_error: None,
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
        match self.state {
            AppState::Login => self.render_login(f),
            AppState::Chat => self.render_chat(f),
        }
    }

    fn render_login(&self, f: &mut Frame) {
        // Clear the background
        let area = f.area();

        let block = Block::default()
            .title(" Welcome to MCS ")
            .borders(Borders::ALL)
            .style(Style::default().bg(Color::Black));
        f.render_widget(block, area);

        let vertical = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(13),
                Constraint::Fill(1),
            ])
            .split(area);

        let popup_layout = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(20),
                Constraint::Percentage(60),
                Constraint::Percentage(20),
            ])
            .split(vertical[1]);

        let popup_area = popup_layout[1];

        f.render_widget(Clear, popup_area);

        let popup_block = Block::default()
            .borders(Borders::ALL)
            .title(" Connect ")
            .style(Style::default().bg(Color::DarkGray));

        let layout = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(3),
            ])
            .split(popup_block.inner(popup_area));

        f.render_widget(popup_block, popup_area);

        let ip_style = if self.login_field_idx == 0 {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let ip_text = Paragraph::new(self.login_ip.as_str())
            .block(Block::default().borders(Borders::ALL).title(" Server IP "))
            .style(ip_style);
        f.render_widget(ip_text, layout[0]);

        let user_style = if self.login_field_idx == 1 {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };

        let user_text = Paragraph::new(self.login_user.as_str())
            .block(Block::default().borders(Borders::ALL).title(" Username "))
            .style(user_style);
        f.render_widget(user_text, layout[1]);

        if let Some(err) = &self.connection_error {
            let error_text =
                Paragraph::new(format!("Error: {err}")).style(Style::default().fg(Color::Red));
            f.render_widget(error_text, layout[2]);
        } else {
            let help_text = Paragraph::new("Tab to switch • Enter to connect • Esc to quit")
                .style(Style::default().fg(Color::Gray));
            f.render_widget(help_text, layout[2]);
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    fn render_chat(&mut self, f: &mut Frame) {
        let input_height = self.get_input_height(f);
        let frame_width = f.area().width.saturating_sub(2) as usize;

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(3), Constraint::Length(input_height)])
            .split(f.area());

        let mut total_lines = 0u16;
        let mut message_lines = Vec::new();
        for m in &self.messages {
            let Some((text, color)) = ChatEvent::to_colored_string(m) else {
                continue;
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

    #[allow(clippy::cast_possible_truncation)]
    const fn get_input_height(&self, f: &Frame) -> u16 {
        let area = f.area();
        let text_width = area.width.saturating_sub(2) as usize;

        let input_lines = if text_width > 0 {
            ((self.input.len() / text_width) as u16) + 1
        } else {
            1
        };

        input_lines + 2
    }

    #[allow(clippy::cast_possible_truncation)]
    const fn get_cursor_position(&self, f: &Frame) -> (u16, u16) {
        let area = f.area();
        let text_width = area.width.saturating_sub(2) as usize;

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
