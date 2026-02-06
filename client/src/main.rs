#![warn(clippy::all, clippy::pedantic, clippy::nursery, unused_extern_crates)]

mod app;
mod error;
mod event;
mod network;
mod tui;
mod ui;

use rustls::crypto::ring;

use crate::{app::App, error::Result};

#[tokio::main]
async fn main() -> Result<()> {
    let _ = ring::default_provider().install_default();

    let mut terminal = tui::init().map_err(error::Error::Io)?;
    let mut events = event::EventHandler::new(250);
    let mut app = App::new(events.sender());

    while !app.global.should_quit {
        terminal
            .draw(|f| ui::render(f, &mut app))
            .map_err(|e| error::Error::Render(e.to_string()))?;
        if let Some(event) = events.next().await {
            app.handle_event(event);
        }
    }

    tui::restore().map_err(error::Error::Io)?;
    Ok(())
}
