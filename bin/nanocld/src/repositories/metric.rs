use diesel::{prelude::*, sql_query};
use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::generic::{GenericClause, GenericFilter};

use crate::{
  gen_sql_multiple, gen_sql_order_by, gen_sql_query,
  models::{
    BalanceWeights, CapacityQuery, ColumnType, MetricDb, MetricNodeDb, NodeDb,
    Pool,
  },
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
  /// Select nodes using a balanced composite score between CPU and Memory pressure.
  /// Applies capacity filters (cpu cores, memory bytes) and utilization caps, and orders by score.
  pub async fn select_nodes_for_capacity_balanced(
    query: &CapacityQuery,
    pool: &Pool,
  ) -> IoResult<Vec<NodeDb>> {
    let pool_ptr = pool.clone();
    // Extract values outside of the blocking closure to avoid lifetime issues
    let cpu_opt: Option<f32> = query.cpu_cores_required.map(|x| x as f32);
    let mem_opt: Option<i64> = query.mem_bytes_required.map(|x| x as i64);
    let cpu_cap_opt: Option<f32> = query.cpu_utilization_cap;
    let mem_cap_opt: Option<f32> = query.mem_utilization_cap;
    let (w_cpu, w_mem) = match &query.weights {
      Some(BalanceWeights {
        cpu_weight,
        mem_weight,
      }) => (*cpu_weight, *mem_weight),
      None => (0.6, 0.4),
    };
    let limit_i64: i64 = query.limit as i64;
    let recency = query.recency_seconds;
    let storage_opt: Option<i64> =
      query.storage_bytes_required.map(|x| x as i64);
    // Clone dynamic constraints to move into blocking closure (as JSONB arrays)
    let require_eq_json = serde_json::Value::Array(
      query
        .require_constraints_eq
        .iter()
        .map(|(path, val)| serde_json::json!({"path": path, "value": val}))
        .collect(),
    );
    let require_ne_json = serde_json::Value::Array(
      query
        .require_constraints_ne
        .iter()
        .map(|(path, val)| serde_json::json!({"path": path, "value": val}))
        .collect(),
    );
    let node_names = ntex::rt::spawn_blocking(move || {
      // Build SQL with dynamic metadata constraints pushed down
      let mut sql_txt = String::from(
        "
          WITH Latest AS (
            SELECT node_name, data,
                   ROW_NUMBER() OVER(PARTITION BY node_name ORDER BY created_at DESC) AS rn
            FROM metrics
            WHERE kind = 'nanocl.io/metrs'
              AND ($9::bigint IS NULL OR created_at >= NOW() - (($9::double precision)::text || ' seconds')::interval)
          ), Stats AS (
            SELECT
              node_name,
              jsonb_array_length(data->'Cpus') AS cores,
              (
                SELECT AVG((c->>'Usage')::double precision)
                FROM jsonb_array_elements(data->'Cpus') c
              ) AS avg_cpu,
              (data->'Memory'->>'Total')::bigint AS mem_total,
              (data->'Memory'->>'Used')::bigint AS mem_used,
              (
                SELECT COALESCE(SUM((d->>'AvailableSpace')::bigint), 0)
                FROM jsonb_array_elements(data->'Disks') d
              ) AS disks_avail_total
            FROM Latest
            WHERE rn = 1
          ), Enriched AS (
            SELECT
              node_name,
              cores,
              avg_cpu,
              mem_total,
              mem_used,
              disks_avail_total,
              CASE WHEN mem_total IS NULL OR mem_total = 0 THEN NULL
                   ELSE (mem_used::double precision / NULLIF(mem_total::double precision, 0.0))
              END AS mem_used_ratio
            FROM Stats
          )
          SELECT n.name AS node_name
          FROM Enriched e
          JOIN nodes n ON n.name = e.node_name
          WHERE
            ($1::double precision IS NULL OR ((1.0 - COALESCE(avg_cpu, 0.0)) * GREATEST(cores, 1)::double precision) >= $1::double precision)
            AND ($2::bigint IS NULL OR (mem_total IS NOT NULL AND mem_used IS NOT NULL AND (mem_total - mem_used) >= $2::bigint))
            AND ($3::double precision IS NULL OR COALESCE(avg_cpu, 0.0) <= $3::double precision)
            AND (
              $4::double precision IS NULL OR (
                mem_total IS NOT NULL AND mem_total > 0 AND
                (COALESCE(mem_used, 0)::double precision / NULLIF(mem_total, 0)::double precision) <= $4::double precision
              )
            )
            AND ($8::bigint IS NULL OR (disks_avail_total IS NOT NULL AND disks_avail_total >= $8::bigint))
        ",
      );
      // Append dynamic constraints via JSONB arrays of {path, value}
      sql_txt.push_str(
        "\n            AND NOT EXISTS (\n              SELECT 1 FROM jsonb_array_elements($10::jsonb) c\n              WHERE NOT (\n                COALESCE(jsonb_extract_path_text(n.metadata, VARIADIC (ARRAY(SELECT jsonb_array_elements_text(c->'path')))) = (c->>'value'), FALSE)\n                OR COALESCE((jsonb_extract_path(n.metadata, VARIADIC (ARRAY(SELECT jsonb_array_elements_text(c->'path')))) ) ? (c->>'value'), FALSE)\n              )\n            )\n            AND NOT EXISTS (\n              SELECT 1 FROM jsonb_array_elements($11::jsonb) c\n              WHERE NOT (\n                COALESCE(jsonb_extract_path_text(n.metadata, VARIADIC (ARRAY(SELECT jsonb_array_elements_text(c->'path')))) IS DISTINCT FROM (c->>'value'), TRUE)\n                AND COALESCE(NOT ((jsonb_extract_path(n.metadata, VARIADIC (ARRAY(SELECT jsonb_array_elements_text(c->'path')))) ) ? (c->>'value')), TRUE)\n              )\n            )",
      );
      // Order and limit
      sql_txt.push_str(
        "\n          ORDER BY
            (COALESCE(avg_cpu, 0.0) * $5::double precision)
            + (COALESCE(mem_used_ratio, 0.0) * $6::double precision) ASC,
            avg_cpu ASC
          LIMIT $7",
      );
  let sql = sql_query(sql_txt);
      let mut conn = utils::store::get_pool_conn(&pool_ptr)?;
      use diesel::sql_types::{BigInt, Float, Jsonb, Nullable};
      // Static binds
      let sql = sql
        .bind::<Nullable<Float>, _>(cpu_opt)
        .bind::<Nullable<BigInt>, _>(mem_opt)
        .bind::<Nullable<Float>, _>(cpu_cap_opt)
        .bind::<Nullable<Float>, _>(mem_cap_opt)
        .bind::<Float, _>(w_cpu)
        .bind::<Float, _>(w_mem)
        .bind::<BigInt, _>(limit_i64)
        .bind::<Nullable<BigInt>, _>(storage_opt)
        .bind::<Nullable<BigInt>, _>(recency)
        .bind::<Jsonb, _>(require_eq_json)
        .bind::<Jsonb, _>(require_ne_json);
      let node_names = sql.get_results::<MetricNodeDb>(&mut conn)
        .map_err(|err| IoError::interrupted("Select nodes for capacity balanced", &err.to_string()))?;
      Ok::<_, IoError>(node_names)
    })
  .await
  .map_err(|err| IoError::interrupted("Select nodes for capacity balanced", &err.to_string()))??;
    let names: Vec<String> =
      node_names.iter().map(|x| x.node_name.clone()).collect();
    let filter = GenericFilter::new()
      .r#where("node_name", GenericClause::In(names.clone()));
    let mut nodes = NodeDb::read_by(&filter, pool).await?;
    use std::collections::HashMap;
    let mut by_name: HashMap<String, NodeDb> =
      nodes.drain(..).map(|n| (n.name.clone(), n)).collect();
    let ordered = names
      .into_iter()
      .filter_map(|n| by_name.remove(&n))
      .collect::<Vec<_>>();
    Ok(ordered)
  }
  /// Select least-loaded nodes that can satisfy optional CPU (cores) and Memory (bytes) requirements.
  /// Uses latest metrics per node and performs filtering/sorting in SQL to avoid loading all nodes.
  pub async fn select_nodes_for_capacity(
    query: &CapacityQuery,
    pool: &Pool,
  ) -> IoResult<Vec<NodeDb>> {
    let pool_ptr = pool.clone();
    // Extract values outside of the blocking closure to avoid lifetime issues
    let cpu_opt: Option<f32> = query.cpu_cores_required.map(|x| x as f32);
    let mem_opt: Option<i64> = query.mem_bytes_required.map(|x| x as i64);
    let cpu_cap_opt: Option<f32> = query.cpu_utilization_cap;
    let mem_cap_opt: Option<f32> = query.mem_utilization_cap;
    let limit_i64: i64 = query.limit as i64;
    let recency = query.recency_seconds;
    let storage_opt: Option<i64> =
      query.storage_bytes_required.map(|x| x as i64);
    let require_eq_json = serde_json::Value::Array(
      query
        .require_constraints_eq
        .iter()
        .map(|(path, val)| serde_json::json!({"path": path, "value": val}))
        .collect(),
    );
    let require_ne_json = serde_json::Value::Array(
      query
        .require_constraints_ne
        .iter()
        .map(|(path, val)| serde_json::json!({"path": path, "value": val}))
        .collect(),
    );
    let node_names = ntex::rt::spawn_blocking(move || {
      // SQL notes:
      // - Latest picks last metric row per node for kind 'nanocl.io/metrs'
      // - Stats computes cores, average CPU usage across cores, and optional memory stats
      // - Filter ensures enough free CPU headroom: (1 - avg_cpu) >= cpu_required/cores
      // - Filter ensures enough free memory if provided
      // - Order by least loaded (avg_cpu ASC) and limit to requested number
      let mut sql_txt = String::from(
        "
          WITH Latest AS (
            SELECT node_name, data,
                   ROW_NUMBER() OVER(PARTITION BY node_name ORDER BY created_at DESC) AS rn
            FROM metrics
            WHERE kind = 'nanocl.io/metrs'
              AND ($7::bigint IS NULL OR created_at >= NOW() - (($7::double precision)::text || ' seconds')::interval)
          ), Stats AS (
            SELECT
              node_name,
              jsonb_array_length(data->'Cpus') AS cores,
              (
                SELECT AVG((c->>'Usage')::double precision)
                FROM jsonb_array_elements(data->'Cpus') c
              ) AS avg_cpu,
              (data->'Memory'->>'Total')::bigint AS mem_total,
              (data->'Memory'->>'Used')::bigint AS mem_used,
              (
                SELECT COALESCE(SUM((d->>'AvailableSpace')::bigint), 0)
                FROM jsonb_array_elements(data->'Disks') d
              ) AS disks_avail_total
            FROM Latest
            WHERE rn = 1
          )
          SELECT n.name AS node_name
          FROM Stats s
          JOIN nodes n ON n.name = s.node_name
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
            AND
            -- CPU utilization cap (skipped when $3 is NULL)
            (
              $3::double precision IS NULL OR COALESCE(avg_cpu, 0.0) <= $3::double precision
            )
            AND
            -- Memory utilization cap (skipped when $4 is NULL)
            (
              $4::double precision IS NULL OR (
                mem_total IS NOT NULL AND mem_total > 0 AND
                (COALESCE(mem_used, 0)::double precision / NULLIF(mem_total, 0)::double precision) <= $4::double precision
              )
            )
            AND ($6::bigint IS NULL OR (disks_avail_total IS NOT NULL AND disks_avail_total >= $6::bigint))
        ",
      );
      // Append dynamic constraints through JSONB arrays ($8 eq, $9 ne)
      sql_txt.push_str(
        "\n            AND NOT EXISTS (\n              SELECT 1 FROM jsonb_array_elements($8::jsonb) c\n              WHERE NOT (\n                COALESCE(jsonb_extract_path_text(n.metadata, VARIADIC (ARRAY(SELECT jsonb_array_elements_text(c->'path')))) = (c->>'value'), FALSE)\n                OR COALESCE((jsonb_extract_path(n.metadata, VARIADIC (ARRAY(SELECT jsonb_array_elements_text(c->'path')))) ) ? (c->>'value'), FALSE)\n              )\n            )\n            AND NOT EXISTS (\n              SELECT 1 FROM jsonb_array_elements($9::jsonb) c\n              WHERE NOT (\n                COALESCE(jsonb_extract_path_text(n.metadata, VARIADIC (ARRAY(SELECT jsonb_array_elements_text(c->'path')))) IS DISTINCT FROM (c->>'value'), TRUE)\n                AND COALESCE(NOT ((jsonb_extract_path(n.metadata, VARIADIC (ARRAY(SELECT jsonb_array_elements_text(c->'path')))) ) ? (c->>'value')), TRUE)\n              )\n            )",
      );
      // Order and limit
      sql_txt.push_str("\n          ORDER BY avg_cpu ASC NULLS LAST\n          LIMIT $5");
  let sql = sql_query(sql_txt);
      let mut conn = utils::store::get_pool_conn(&pool_ptr)?;
      use diesel::sql_types::{BigInt, Float, Jsonb, Nullable};
      // Static binds first in order
      let sql = sql
        .bind::<Nullable<Float>, _>(cpu_opt)
        .bind::<Nullable<BigInt>, _>(mem_opt)
        .bind::<Nullable<Float>, _>(cpu_cap_opt)
        .bind::<Nullable<Float>, _>(mem_cap_opt)
        .bind::<BigInt, _>(limit_i64)
        .bind::<Nullable<BigInt>, _>(storage_opt)
        .bind::<Nullable<BigInt>, _>(recency)
        .bind::<Jsonb, _>(require_eq_json)
        .bind::<Jsonb, _>(require_ne_json);
      let node_names = sql.get_results::<MetricNodeDb>(&mut conn)
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
