use std::io::Stdout;

use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{Terminal, prelude::CrosstermBackend};

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

pub fn init() -> std::io::Result<Tui> {
    crossterm::terminal::enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Terminal::new(backend)
}

pub fn restore() -> std::io::Result<()> {
    crossterm::terminal::disable_raw_mode()?;
    execute!(std::io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}
