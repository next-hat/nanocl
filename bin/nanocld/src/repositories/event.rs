use crate::{
  gen_multiple, gen_where4uuid, gen_where4string, models::EventDb,
  schema::events,
};

use diesel::prelude::*;
use nanocl_error::io::IoResult;
use nanocl_stubs::system::Event;

use super::generic::*;

impl RepositoryBase for EventDb {}

impl RepositoryCreate for EventDb {}

impl RepositoryReadBy for EventDb {
  type Output = EventDb;

  fn get_pk() -> &'static str {
    "key"
  }

  fn gen_read_query(
    filter: &nanocl_stubs::generic::GenericFilter,
    is_multiple: bool,
  ) -> impl diesel::query_dsl::methods::LoadQuery<
    'static,
    diesel::PgConnection,
    Self::Output,
  >
  where
    Self::Output: Sized,
  {
    let r#where = filter.r#where.to_owned().unwrap_or_default();
    let mut query = events::table.into_boxed();
    if let Some(value) = r#where.get("key") {
      gen_where4uuid!(query, events::key, value);
    }
    if let Some(value) = r#where.get("reporting_node") {
      gen_where4string!(query, events::reporting_node, value);
    }
    if let Some(value) = r#where.get("kind") {
      gen_where4string!(query, events::kind, value);
    }
    if let Some(value) = r#where.get("action") {
      gen_where4string!(query, events::kind, value);
    }
    if let Some(value) = r#where.get("reason") {
      gen_where4string!(query, events::kind, value);
    }
    if is_multiple {
      gen_multiple!(query, events::created_at, filter);
    }
    query
  }
}

impl RepositoryReadByTransform for EventDb {
  type NewOutput = Event;

  fn transform(input: Self::Output) -> IoResult<Self::NewOutput> {
    Self::NewOutput::try_from(input)
  }
}
