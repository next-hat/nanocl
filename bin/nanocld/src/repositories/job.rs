use diesel::prelude::*;

use nanocl_stubs::generic::GenericFilter;

use crate::{
  models::{JobDb, JobUpdateDb},
  schema::jobs,
};

use super::generic::*;

impl RepositoryBase for JobDb {}

impl RepositoryCreate for JobDb {}

impl RepositoryUpdate for JobDb {
  type UpdateItem = JobUpdateDb;
}

impl RepositoryDelByPk for JobDb {}

impl RepositoryRead for JobDb {
  type Output = JobDb;
  type Query = jobs::BoxedQuery<'static, diesel::pg::Pg>;

  fn gen_read_query(filter: &GenericFilter, is_multiple: bool) -> Self::Query {
    let mut query = jobs::dsl::jobs.into_boxed();
    if is_multiple {
      query = query.order(jobs::dsl::created_at.desc());
      let limit = filter.limit.unwrap_or(100);
      query = query.limit(limit as i64);
      if let Some(offset) = filter.offset {
        query = query.offset(offset as i64);
      }
    }
    query
  }
}
