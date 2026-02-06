use crate::app::{App, CurrentScreen};
use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
};

pub mod components;
pub mod screens;

/// Delegates control to specific screens based on app state.
pub fn render(f: &mut Frame, app: &mut App) {
    match app.global.screen {
        CurrentScreen::Login => screens::login::draw(f, app),
        CurrentScreen::Chat => screens::chat::draw(f, app),
    }
}

pub fn centered_rect(area: Rect, percent_x: u16, percent_y: u16) -> Rect {
    let popout_layout = Layout::default()
        .direction(ratatui::layout::Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    let vertical_center = popout_layout[1];
    let horizontal_layout = Layout::default()
        .direction(ratatui::layout::Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical_center);

    horizontal_layout[1]
}
