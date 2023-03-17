use serde::{Serialize, Deserialize};

use crate::schema::nodes;

#[derive(
  Debug, Clone, Queryable, Identifiable, Insertable, Serialize, Deserialize,
)]
#[diesel(primary_key(name))]
#[diesel(table_name = nodes)]
pub struct NodeDbModel {
  pub(crate) name: String,
  pub(crate) ip_address: String,
}
