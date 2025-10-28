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
  fn get_columns<'a>()
  -> std::collections::HashMap<&'a str, (ColumnType, &'a str)> {
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
  /// Select least-loaded nodes that can satisfy optional CPU (cores) and Memory (bytes) requirements.
  /// Uses latest metrics per node and performs filtering/sorting in SQL to avoid loading all nodes.
  pub async fn select_nodes_for_capacity(
    cpu_cores_required: Option<usize>,
    mem_bytes_required: Option<usize>,
    limit: usize,
    pool: &Pool,
  ) -> IoResult<Vec<NodeDb>> {
    let pool_ptr = pool.clone();
    let node_names = ntex::rt::spawn_blocking(move || {
      // SQL notes:
      // - Latest picks last metric row per node for kind 'nanocl.io/metrs'
      // - Stats computes cores, average CPU usage across cores, and optional memory stats
      // - Filter ensures enough free CPU headroom: (1 - avg_cpu) >= cpu_required/cores
      // - Filter ensures enough free memory if provided
      // - Order by least loaded (avg_cpu ASC) and limit to requested number
      let query = sql_query(
        "
          WITH Latest AS (
            SELECT node_name, data,
                   ROW_NUMBER() OVER(PARTITION BY node_name ORDER BY created_at DESC) AS rn
            FROM metrics
            WHERE kind = 'nanocl.io/metrs'
          ), Stats AS (
            SELECT
              node_name,
              jsonb_array_length(data->'Cpus') AS cores,
              (
                SELECT AVG((c->>'Usage')::double precision)
                FROM jsonb_array_elements(data->'Cpus') c
              ) AS avg_cpu,
              (data->'Memory'->>'Total')::bigint AS mem_total,
              (data->'Memory'->>'Used')::bigint AS mem_used
            FROM Latest
            WHERE rn = 1
          )
          SELECT node_name
          FROM Stats
          WHERE
            -- CPU headroom condition (skipped when $1 is NULL)
            (
              $1::double precision IS NULL OR
              ((1.0 - COALESCE(avg_cpu, 0.0)) * GREATEST(cores, 1)::double precision) >= $1::double precision
            )
            AND
            -- Memory free condition (skipped when $2 is NULL)
            (
              $2::bigint IS NULL OR
              (mem_total IS NOT NULL AND mem_used IS NOT NULL AND (mem_total - mem_used) >= $2::bigint)
            )
          ORDER BY avg_cpu ASC NULLS LAST
          LIMIT $3
        ",
      );
      let mut conn = utils::store::get_pool_conn(&pool_ptr)?;
      use diesel::sql_types::{BigInt, Float, Nullable};
      let cpu_opt: Option<f32> = cpu_cores_required.map(|x| x as f32);
      let mem_opt: Option<i64> = mem_bytes_required.map(|x| x as i64);
      let node_names = query
        .bind::<Nullable<Float>, _>(cpu_opt)
        .bind::<Nullable<BigInt>, _>(mem_opt)
        .bind::<BigInt, _>(limit as i64)
        .get_results::<MetricNodeDb>(&mut conn)
        .map_err(|err| IoError::interrupted("Select nodes for capacity", &err.to_string()))?;
      Ok::<_, IoError>(node_names)
    })
    .await
    .map_err(|err| IoError::interrupted("Select nodes for capacity", &err.to_string()))??;
    let filter = GenericFilter::new().r#where(
      "node_name",
      GenericClause::In(
        node_names.iter().map(|x| x.node_name.clone()).collect(),
      ),
    );
    NodeDb::read_by(&filter, pool).await
  }
}
