use std::sync::Arc;

use nanocld_client::NanocldClient;

use super::Dnsmasq;

pub struct SystemState {
  pub client: NanocldClient,
  pub dnsmasq: Dnsmasq,
}

pub type SystemStateRef = Arc<SystemState>;
