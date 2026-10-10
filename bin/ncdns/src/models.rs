use nanocld_client::NanocldClient;

use crate::dnsmasq::Dnsmasq;

/// Application state shared by the DNS controller handlers.
#[derive(Clone)]
pub struct SystemState {
  pub dnsmasq: Dnsmasq,
  pub client: NanocldClient,
}

impl ntex::web::State for SystemState {
  type Error = ntex::web::DefaultError;
}
