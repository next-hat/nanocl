use std::{
  collections::HashMap,
  time::{UNIX_EPOCH, SystemTime},
};
use nanocl_error::io::{IoResult, FromIo};

use nanocld_client::NanocldClient;

use crate::utils::secret::update_secret_cert;

const RENEW_BEFORE_EXPIRY_DURATION: u64 = 60 * 60 * 24 * 2;

pub struct NCertManager<'a> {
  secrets_map: HashMap<String, u64>,
  pub state_dir: String,
  pub cert_dir: String,
  pub client: &'a NanocldClient,
}

impl<'a> NCertManager<'a> {
  pub fn new(
    client: &'a NanocldClient,
    state_dir: String,
    cert_dir: String,
  ) -> NCertManager<'a> {
    NCertManager {
      cert_dir,
      state_dir,
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

  pub fn add_secret(&mut self, key: String, expiry: u64) {
    self.secrets_map.insert(key, expiry);
  }

  pub fn remove_secret(&mut self, key: &String) {
    self.secrets_map.remove(key);
  }

  fn get_renewable_secrets(&self) -> Vec<String> {
    let mut renewables_secrets = Vec::new();

    for (key, expiry) in &self.secrets_map {
      match NCertManager::is_renew_date_past(expiry) {
        Err(err) => {
          log::error!("Can't compute expiry date: {err}");
        }
        Ok(should_renew) => {
          if should_renew {
            renewables_secrets.push(key.to_owned());
          }
        }
      }
    }

    renewables_secrets
  }

  pub async fn renew_secrets(&self) {
    let secrets = self.get_renewable_secrets();

    for secret_key in secrets {
      if let Err(err) = update_secret_cert(
        self.client,
        secret_key.to_owned(),
        self.cert_dir.to_owned(),
        self.state_dir.to_owned(),
      )
      .await
      {
        log::error!("Can't update secret {}: {}", secret_key, err);
      };
    }
  }

  pub fn debug(&self) {
    log::info!("{:?}", self.secrets_map)
  }
}
