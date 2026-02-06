use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyEvent, KeyEventKind};
use futures::StreamExt;
use protocol::Message;
use tokio::{sync::mpsc, task::JoinHandle};

use crate::error::Error;

/// Represents an app event.
#[derive(Debug)]
pub enum AppEvent {
    /// Input from the user.
    Input(KeyEvent),
    /// Network packet received from the server.
    Network(Message),
    /// Internal errors
    Err(Error),
    /// Periodic UI redraw ticks
    Tick,
    LoginSuccess(mpsc::UnboundedSender<Message>),
    LoginFailed(String),
}

/// A wrapper to drive the event loop.
pub struct EventHandler {
    /// Sender to dispatch internal events.
    sender: mpsc::UnboundedSender<AppEvent>,
    /// Receiver for the main loop.
    receiver: mpsc::UnboundedReceiver<AppEvent>,
    /// Handle to the tick task.
    _tick_task: JoinHandle<()>,
    /// Handle to the input task.
    _input_task: JoinHandle<()>,
}

impl EventHandler {
    pub fn new(tick_rate: u64) -> Self {
        let (sender, receiver) = mpsc::unbounded_channel();

        let tick_sender = sender.clone();
        let tick_task = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_millis(tick_rate));
            loop {
                interval.tick().await;
                if tick_sender.send(AppEvent::Tick).is_err() {
                    break;
                }
            }
        });

        let input_sender = sender.clone();
        let input_task = tokio::spawn(async move {
            let mut reader = EventStream::new();
            while let Some(Ok(event)) = reader.next().await
                && let Event::Key(key) = event
                && key.kind == KeyEventKind::Press
            {
                if input_sender.send(AppEvent::Input(key)).is_err() {
                    break;
                }
            }
        });

        Self {
            sender,
            receiver,
            _input_task: tick_task,
            _tick_task: input_task,
        }
    }

    // Returns a clone of the sender that other threads can use to inject events.
    pub fn sender(&self) -> mpsc::UnboundedSender<AppEvent> {
        self.sender.clone()
    }

    /// Wait for the next event.
    pub async fn next(&mut self) -> Option<AppEvent> {
        self.receiver.recv().await
    }
}
