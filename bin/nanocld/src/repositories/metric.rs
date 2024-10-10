use diesel::{prelude::*, sql_query};
use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::generic::{GenericClause, GenericFilter};

use crate::{
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  models::{ColumnType, MetricDb, MetricNodeDb, NodeDb, Pool},
  schema::metrics,
  utils,
};

use super::generic::*;

impl RepositoryBase for MetricDb {
  fn get_columns<'a>(
  ) -> std::collections::HashMap<&'a str, (ColumnType, &'a str)> {
    std::collections::HashMap::from([
      ("key", (ColumnType::Uuid, "metrics.key")),
      ("node_name", (ColumnType::Text, "metrics.node_name")),
      ("kind", (ColumnType::Text, "metrics.kind")),
      ("data", (ColumnType::Json, "metrics.data")),
      (
        "created_at",
        (ColumnType::Timestamptz, "metrics.created_at"),
      ),
    ])
  }
}

impl RepositoryCreate for MetricDb {}

impl RepositoryReadBy for MetricDb {
  type Output = MetricDb;

  fn get_pk() -> &'static str {
    "key"
  }

  fn gen_read_query(
    filter: &GenericFilter,
    is_multiple: bool,
  ) -> impl diesel::query_dsl::methods::LoadQuery<
    'static,
    diesel::pg::PgConnection,
    Self::Output,
  > {
    let mut query = metrics::table.into_boxed();
    let columns = Self::get_columns();
    query = gen_sql_query!(query, filter, columns);
    if let Some(orders) = &filter.order_by {
      query = gen_sql_order_by!(query, orders, columns);
    } else {
      query = query.order(metrics::created_at.desc());
    }
    if is_multiple {
      gen_sql_multiple!(query, filter);
    }
    query
  }
}

impl RepositoryCountBy for MetricDb {
  fn gen_count_query(
    filter: &GenericFilter,
  ) -> impl diesel::query_dsl::LoadQuery<'static, diesel::PgConnection, i64> {
    let mut query = metrics::table.into_boxed();
    let columns = Self::get_columns();
    gen_sql_query!(query, filter, columns).count()
  }
}

impl MetricDb {
  pub async fn find_best_nodes(
    cpu_threshold: f32,
    _memory_threshold: f32,
    limit: usize,
    pool: &Pool,
  ) -> IoResult<Vec<NodeDb>> {
    let pool_ptr = pool.clone();
    let node_names = ntex::rt::spawn_blocking(move || {
      let query = sql_query(
        "
          WITH LatestMetrics AS (
            SELECT
              node_name,
              data,
              ROW_NUMBER() OVER(PARTITION BY node_name ORDER BY created_at DESC) AS rn
            FROM metrics
            WHERE kind = 'nanocl.io/metrs'
          ), CpuUsages AS (
            SELECT
              node_name,
              jsonb_array_elements(data->'Cpus') AS cpu
            FROM LatestMetrics
            WHERE rn = 1
          )
          SELECT
            node_name,
            AVG((cpu->>'Usage')::float) AS avg_cpu_usage
          FROM CpuUsages
          GROUP BY node_name
          HAVING AVG((cpu->>'Usage')::float) >= $1
          ORDER BY avg_cpu_usage DESC
          LIMIT $2
        ",
      );
      let mut conn = utils::store::get_pool_conn(&pool_ptr)?;
      let node_names = query
        .bind::<diesel::sql_types::Float, _>(cpu_threshold)
        .bind::<diesel::sql_types::BigInt, _>(limit as i64)
        .get_results::<MetricNodeDb>(&mut conn).map_err(|err| {
          IoError::interrupted("Find best node", &err.to_string())
        })?;
      Ok::<_, IoError>(node_names)
    })
    .await
    .map_err(|err| {
      IoError::interrupted("Find best node", &err.to_string())
    })??;
    let filter = GenericFilter::new().r#where(
      "node_name",
      GenericClause::In(
        node_names.iter().map(|x| x.node_name.clone()).collect(),
      ),
    );
    NodeDb::read_by(&filter, pool).await
  }
}
