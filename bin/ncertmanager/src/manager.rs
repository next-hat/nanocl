use std::{
  collections::HashMap,
  time::{UNIX_EPOCH, SystemTime},
};
use nanocl_error::io::{IoResult, FromIo};

use nanocld_client::NanocldClient;

const RENEW_BEFORE_EXPIRY_DURATION: u64 = 60 * 60 * 24 * 2;

pub struct NCertManager {
  secrets_map: HashMap<String, u64>,
  pub client: NanocldClient,
}

impl NCertManager {
  pub fn new(client: NanocldClient) -> NCertManager {
    NCertManager {
      client,
      secrets_map: HashMap::new(),
    }
  }

  pub fn is_renew_date_past(expiry: &u64) -> IoResult<bool> {
    Ok(
      *expiry
        < RENEW_BEFORE_EXPIRY_DURATION
          + SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| err.map_err_context(|| "Get time"))?
            .as_secs(),
    )
  }

  pub fn should_renew(&self, key: &String) -> IoResult<bool> {
    let expiry = self.secrets_map.get(key).unwrap_or(&0);

    NCertManager::is_renew_date_past(expiry)
  }

  pub fn add_secret(&mut self, key: String, expiry: u64) {
    self.secrets_map.insert(key, expiry);
  }

  pub fn remove_secret(&mut self, key: &String) {
    self.secrets_map.remove(key);
  }

  pub fn get_renewable_secrets(&self) -> Vec<String> {
    let mut renewables_secrets = Vec::new();

    for (key, expiry) in &self.secrets_map {
      if NCertManager::is_renew_date_past(expiry).unwrap() {
        renewables_secrets.push(key.to_owned());
      }
    }

    renewables_secrets
  }

  pub fn debug(&self) {
    log::info!("{:?}", self.secrets_map)
  }
}
