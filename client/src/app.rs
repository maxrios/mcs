use crate::{error::Error, event::AppEvent, network::NetworkClient};
use crossterm::event::{KeyCode, KeyEvent};
use protocol::{ChatPacket, JoinPacket, Message};
use std::collections::VecDeque;
use tokio::sync::mpsc;

/// Maximum number of messages to keep in memory.
const MAX_MESSAGES: usize = 500;

/// Actions to be handled by the app.
pub enum Action {
    /// User input containing a character.
    EnterChar(char),
    /// User input deleting a character.
    DeleteChar,
    /// Message to be sent to the server.
    Submit,
    /// User closes the app.
    Quit,
    None,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum CurrentScreen {
    Login,
    Chat,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LoginStep {
    Ip,
    Username,
    Password,
}

pub struct GlobalState {
    pub screen: CurrentScreen,
    pub should_quit: bool,
    pub event_tx: mpsc::UnboundedSender<AppEvent>,
}

pub struct UIState {
    pub input_buffer: String,
    pub error_message: Option<String>,
}

pub struct ChatState {
    pub messages: VecDeque<ChatPacket>,
    pub network: Option<NetworkClient>,
    pub username: String,
}

pub struct LoginState {
    pub step: LoginStep,
    pub ip: String,
    pub user: String,
}

pub struct App {
    pub global: GlobalState,
    pub ui: UIState,
    pub chat: ChatState,
    pub login: LoginState,
}

impl App {
    pub fn new(event_tx: mpsc::UnboundedSender<AppEvent>) -> Self {
        Self {
            global: GlobalState {
                screen: CurrentScreen::Login,
                should_quit: false,
                event_tx,
            },
            ui: UIState {
                input_buffer: String::new(),
                error_message: None,
            },
            chat: ChatState {
                messages: VecDeque::with_capacity(MAX_MESSAGES),
                network: None,
                username: String::new(),
            },
            login: LoginState {
                step: LoginStep::Ip,
                ip: String::new(),
                user: String::new(),
            },
        }
    }

    /// Consumes an event and updates state.
    pub fn handle_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Input(key) => {
                let action = Self::map_key_to_action(key);
                self.dispatch_action(&action);
            }
            AppEvent::Network(msg) => {
                self.process_network_message(msg);
            }
            AppEvent::Err(e) => {
                self.handle_error(&e);
            }
            AppEvent::Tick => {}
            AppEvent::LoginSuccess(tx) => {
                self.chat.network = Some(NetworkClient::new(tx));
                self.chat.username = self.login.user.clone();
                self.global.screen = CurrentScreen::Chat;
                self.ui.error_message = None;
            }
            AppEvent::LoginFailed(e) => {
                self.ui.error_message = Some(format!("Connection failed: {e}"));
            }
        }
    }

    const fn map_key_to_action(key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => Action::Quit,
            KeyCode::Enter => Action::Submit,
            KeyCode::Backspace => Action::DeleteChar,
            KeyCode::Char(c) => Action::EnterChar(c),
            _ => Action::None,
        }
    }

    fn dispatch_action(&mut self, action: &Action) {
        if !matches!(action, Action::None) {
            self.ui.error_message = None;
        }

        match action {
            Action::Quit => self.global.should_quit = true,
            Action::EnterChar(c) => self.ui.input_buffer.push(*c),
            Action::DeleteChar => {
                let _ = self.ui.input_buffer.pop();
            }
            Action::Submit => self.handle_submit(),
            Action::None => {}
        }
    }

    fn handle_submit(&mut self) {
        let input = std::mem::take(&mut self.ui.input_buffer);

        match self.global.screen {
            CurrentScreen::Login => self.handle_login_submit(input),
            CurrentScreen::Chat => self.handle_chat_submit(input),
        }
    }

    fn handle_login_submit(&mut self, input: String) {
        if input.trim().is_empty() {
            return;
        }

        match self.login.step {
            LoginStep::Ip => {
                self.login.ip = input;
                self.login.step = LoginStep::Username;
            }
            LoginStep::Username => {
                self.login.user = input;
                self.login.step = LoginStep::Password;
            }
            LoginStep::Password => {
                let password = input;
                self.connect_to_server(password);
            }
        }
    }

    fn connect_to_server(&mut self, password: String) {
        self.ui.error_message = Some("Connecting...".to_string());

        let ip = self.login.ip.clone();
        let user = self.login.user.clone();
        let event_tx = self.global.event_tx.clone();

        tokio::spawn(async move {
            match NetworkClient::connect(&ip, event_tx.clone()).await {
                Ok(client) => {
                    let join_packet = Message::Join(JoinPacket {
                        username: user,
                        password,
                    });

                    if let Err(e) = client.send(join_packet) {
                        let _ =
                            event_tx.send(AppEvent::LoginFailed(format!("Handshake failed: {e}")));
                        return;
                    }

                    let _ = event_tx.send(AppEvent::LoginSuccess(client.into_inner()));
                }
                Err(e) => {
                    let _ = event_tx.send(AppEvent::LoginFailed(e.to_string()));
                }
            }
        });
    }

    fn handle_chat_submit(&mut self, input: String) {
        if input.trim().is_empty() {
            return;
        }

        if let Some(network) = &self.chat.network {
            let packet = ChatPacket::new_user_packet(self.chat.username.clone(), input);
            let msg = Message::Chat(packet);

            if let Err(e) = network.send(msg) {
                self.handle_error(&e);
            }
        } else {
            self.ui.error_message = Some("Disconnected from server".to_string());
        }
    }

    fn process_network_message(&mut self, msg: Message) {
        match msg {
            Message::Chat(packet) => {
                self.push_message(packet);
            }
            Message::HistoryResponse(history) => {
                for packet in history {
                    self.push_message(packet);
                }
            }
            Message::Error(e) => {
                self.ui.error_message = Some(format!("Server error: {e}"));
            }
            _ => {}
        }
    }

    fn handle_error(&mut self, err: &Error) {
        match err {
            Error::Disconnected => {
                self.ui.error_message = Some("Connection lost. Press Esc to quit".to_string());
                self.chat.network = None;
                self.global.screen = CurrentScreen::Login;
            }
            _ => {
                self.ui.error_message = Some(format!("Error: {err}"));
            }
        }
    }

    fn push_message(&mut self, packet: ChatPacket) {
        if self.chat.messages.len() >= MAX_MESSAGES {
            self.chat.messages.pop_front();
        }
        self.chat.messages.push_back(packet);
    }
}
