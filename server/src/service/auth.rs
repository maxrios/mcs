use crate::error::{Error, Result};
use crate::repository::{PresenceRepository, UserRepository};
use std::sync::Arc;

#[derive(Clone)]
pub struct AuthService {
    users: Arc<dyn UserRepository>,
    presence: Arc<dyn PresenceRepository>,
}

impl AuthService {
    pub fn new(users: Arc<dyn UserRepository>, presence: Arc<dyn PresenceRepository>) -> Self {
        Self { users, presence }
    }

    pub async fn register_and_login(&self, username: &str, password: &str) -> Result<()> {
        if username.trim().len() < 3 {
            return Err(Error::UsernameTooShort(username.to_string()));
        }

        let is_valid = self.users.verify_credentials(username, password).await?;
        if !is_valid {
            self.users.create_user(username, password).await?;
            if !self.users.verify_credentials(username, password).await? {
                return Err(Error::InvalidCredentials);
            }
        }

        if !self.presence.set_online(username).await? {
            return Err(Error::UsernameTaken(
                "user is already logged in".to_string(),
            ));
        }

        Ok(())
    }

    pub async fn logout(&self, username: &str) -> Result<()> {
        self.presence.set_offline(username).await
    }

    pub async fn refresh_session(&self, username: &str) -> Result<()> {
        self.presence.refresh_heartbeat(username).await
    }
}
