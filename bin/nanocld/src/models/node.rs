use nanocld_client::NanocldClient;
use serde::{Serialize, Deserialize};

use crate::schema::nodes;

#[derive(
  Debug, Clone, Queryable, Identifiable, Insertable, Serialize, Deserialize,
)]
#[diesel(primary_key(name))]
#[diesel(table_name = nodes)]
#[serde(rename_all = "PascalCase")]
pub struct NodeDbModel {
  pub(crate) name: String,
  pub(crate) ip_address: String,
}

impl NodeDbModel {
  pub fn to_http_client(&self) -> NanocldClient {
    let url =
      Box::leak(format!("http://{}:8081", self.ip_address).into_boxed_str());

    NanocldClient::connect_to(url, None)
  }
}
