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

fn capacity_node_filter(names: &[String]) -> GenericFilter {
  GenericFilter::new()
    .r#where("name", GenericClause::In(names.to_vec()))
    .limit(names.len())
}

fn order_capacity_nodes(names: Vec<String>, nodes: Vec<NodeDb>) -> Vec<NodeDb> {
  let mut by_name = nodes
    .into_iter()
    .map(|node| (node.name.clone(), node))
    .collect::<std::collections::HashMap<_, _>>();
  names
    .into_iter()
    .filter_map(|name| by_name.remove(&name))
    .collect()
}

async fn hydrate_capacity_nodes(
  selected: Vec<MetricNodeDb>,
  pool: &Pool,
) -> IoResult<Vec<NodeDb>> {
  if selected.is_empty() {
    return Ok(Vec::new());
  }
  let names = selected
    .into_iter()
    .map(|node| node.node_name)
    .collect::<Vec<_>>();
  let nodes = NodeDb::read_by(&capacity_node_filter(&names), pool).await?;
  Ok(order_capacity_nodes(names, nodes))
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
        "\n            AND NOT EXISTS (\n              SELECT 1 FROM jsonb_array_elements($10::jsonb) c\n              WHERE NOT (\n                COALESCE((n.metadata #>> ARRAY(SELECT jsonb_array_elements_text(c->'path'))) = (c->>'value'), FALSE)\n                OR COALESCE(((n.metadata #> ARRAY(SELECT jsonb_array_elements_text(c->'path'))) ? (c->>'value')), FALSE)\n              )\n            )\n            AND NOT EXISTS (\n              SELECT 1 FROM jsonb_array_elements($11::jsonb) c\n              WHERE NOT (\n                COALESCE((n.metadata #>> ARRAY(SELECT jsonb_array_elements_text(c->'path'))) IS DISTINCT FROM (c->>'value'), TRUE)\n                AND COALESCE(NOT ((n.metadata #> ARRAY(SELECT jsonb_array_elements_text(c->'path'))) ? (c->>'value')), TRUE)\n              )\n            )",
      );
      // Order and limit
      sql_txt.push_str(
        "\n          ORDER BY
            (COALESCE(avg_cpu, 0.0) * $5::double precision)
            + (COALESCE(mem_used_ratio, 0.0) * $6::double precision) ASC,
            avg_cpu ASC
          LIMIT $7",
      );
  // Debug: log the generated SQL for troubleshooting (no parameters included)
  log::debug!("[balanced] capacity SQL:\n{}", sql_txt);
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
    hydrate_capacity_nodes(node_names, pool).await
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
        "\n            AND NOT EXISTS (\n              SELECT 1 FROM jsonb_array_elements($8::jsonb) c\n              WHERE NOT (\n                COALESCE((n.metadata #>> ARRAY(SELECT jsonb_array_elements_text(c->'path'))) = (c->>'value'), FALSE)\n                OR COALESCE(((n.metadata #> ARRAY(SELECT jsonb_array_elements_text(c->'path'))) ? (c->>'value')), FALSE)\n              )\n            )\n            AND NOT EXISTS (\n              SELECT 1 FROM jsonb_array_elements($9::jsonb) c\n              WHERE NOT (\n                COALESCE((n.metadata #>> ARRAY(SELECT jsonb_array_elements_text(c->'path'))) IS DISTINCT FROM (c->>'value'), TRUE)\n                AND COALESCE(NOT ((n.metadata #> ARRAY(SELECT jsonb_array_elements_text(c->'path'))) ? (c->>'value')), TRUE)\n              )\n            )",
      );
      // Order and limit
      sql_txt.push_str("\n          ORDER BY avg_cpu ASC NULLS LAST\n          LIMIT $5");
  // Debug: log the generated SQL for troubleshooting (no parameters included)
  log::debug!("[least-loaded] capacity SQL:\n{}", sql_txt);
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
    hydrate_capacity_nodes(node_names, pool).await
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn node(name: &str, created_at: chrono::NaiveDateTime) -> NodeDb {
    NodeDb {
      name: name.to_owned(),
      created_at,
      endpoint: "http://127.0.0.1:0".to_owned(),
      version: crate::vars::VERSION.to_owned(),
      metadata: None,
    }
  }

  #[test]
  fn capacity_hydration_uses_supported_name_and_complete_selection_limit() {
    let names = (0..103).map(|i| format!("node-{i}")).collect::<Vec<_>>();
    let filter = capacity_node_filter(&names);
    assert_eq!(filter.limit, Some(103));
    // Exercise the same Diesel macros and accepted column map as NodeDb reads.
    let mut query = crate::schema::nodes::table.into_boxed();
    query = gen_sql_query!(query, &filter, NodeDb::get_columns());
    gen_sql_multiple!(query, &filter);
    let sql = diesel::debug_query::<diesel::pg::Pg, _>(&query).to_string();
    assert!(sql.contains("WHERE (nodes.name = ANY("), "{sql}");
    assert!(sql.contains("LIMIT $2 OFFSET $3"), "{sql}");
    assert!(sql.ends_with(", 103, 0]"), "{sql}");
    let conditions = filter.r#where.unwrap().conditions;
    assert_eq!(conditions.len(), 1);
    assert!(NodeDb::get_columns().contains_key("name"));
    assert!(
      matches!(conditions.get("name"), Some(GenericClause::In(selected)) if selected == &names)
    );
  }

  #[test]
  fn capacity_hydration_preserves_ranking_and_never_substitutes_nodes() {
    let now = chrono::Utc::now().naive_utc();
    let selected = vec![
      "older-low-load".to_owned(),
      "missing".to_owned(),
      "newer-high-load".to_owned(),
    ];
    let nodes = vec![
      node("ineligible", now),
      node("newer-high-load", now),
      node("older-low-load", now - chrono::Duration::hours(1)),
    ];
    let ordered = order_capacity_nodes(selected, nodes);
    assert_eq!(
      ordered
        .iter()
        .map(|node| node.name.as_str())
        .collect::<Vec<_>>(),
      vec!["older-low-load", "newer-high-load"]
    );
    assert!(order_capacity_nodes(Vec::new(), ordered).is_empty());
  }

  // Database/daemon regression: compile locally, execute with CI infrastructure.
  #[ntex::test]
  async fn capacity_selection_hydrates_only_ranked_eligible_nodes_beyond_one_page()
   {
    use crate::repositories::operation::run_query_owned;
    use crate::schema::nodes;

    let system = crate::utils::tests::gen_default_test_system().await;
    let pool = &system.state.inner.pool;
    let group = format!("capacity-{}", uuid::Uuid::new_v4());
    let now = chrono::Utc::now().naive_utc();
    let mut expected = Vec::new();
    let mut inserted = Vec::new();
    // The lowest-load node is also oldest; an ineligible node is newest.
    for index in 0..104 {
      let name = format!("{group}-{index}");
      let mut record =
        node(&name, now - chrono::Duration::seconds(104 - index));
      record.metadata = Some(serde_json::json!({
        "capacity_test": if index < 103 { group.clone() } else { format!("{group}-ineligible") },
      }));
      NodeDb::create_from(record, pool).await.unwrap();
      MetricDb::create_from(
        MetricDb {
          key: uuid::Uuid::new_v4(),
          created_at: now,
          expires_at: now + chrono::Duration::hours(1),
          node_name: name.clone(),
          kind: "nanocl.io/metrs".to_owned(),
          note: None,
          data: serde_json::json!({
            "Cpus": [{"Usage": index as f64 / 200.0}],
            "Memory": {"Total": 1000000, "Used": 1000},
            "Disks": [{"AvailableSpace": 1000000}],
          }),
        },
        pool,
      )
      .await
      .unwrap();
      inserted.push(name.clone());
      if index < 103 {
        expected.push(name);
      }
    }
    let mut query = CapacityQuery {
      cpu_cores_required: None,
      mem_bytes_required: None,
      storage_bytes_required: None,
      cpu_utilization_cap: None,
      mem_utilization_cap: None,
      recency_seconds: Some(300),
      limit: 103,
      weights: None,
      require_constraints_eq: vec![(
        vec!["capacity_test".to_owned()],
        group.clone(),
      )],
      require_constraints_ne: Vec::new(),
    };
    for balanced in [false, true] {
      let selected = if balanced {
        MetricDb::select_nodes_for_capacity_balanced(&query, pool).await
      } else {
        MetricDb::select_nodes_for_capacity(&query, pool).await
      }
      .unwrap();
      assert_eq!(
        selected
          .into_iter()
          .map(|node| node.name)
          .collect::<Vec<_>>(),
        expected
      );
    }
    // A sole eligible old node must not disappear behind the newest 100 rows.
    query.cpu_utilization_cap = Some(0.001);
    for balanced in [false, true] {
      let selected = if balanced {
        MetricDb::select_nodes_for_capacity_balanced(&query, pool).await
      } else {
        MetricDb::select_nodes_for_capacity(&query, pool).await
      }
      .unwrap();
      assert_eq!(
        selected
          .into_iter()
          .map(|node| node.name)
          .collect::<Vec<_>>(),
        vec![expected[0].clone()]
      );
    }
    query.require_constraints_eq[0].1 = format!("{group}-absent");
    assert!(
      MetricDb::select_nodes_for_capacity(&query, pool)
        .await
        .unwrap()
        .is_empty()
    );
    assert!(
      MetricDb::select_nodes_for_capacity_balanced(&query, pool)
        .await
        .unwrap()
        .is_empty()
    );
    run_query_owned(pool, (), move |conn| {
      diesel::delete(
        metrics::table.filter(metrics::node_name.eq_any(&inserted)),
      )
      .execute(conn)
      .map_err(MetricDb::map_err)?;
      diesel::delete(nodes::table.filter(nodes::name.eq_any(inserted)))
        .execute(conn)
        .map_err(NodeDb::map_err)?;
      Ok(())
    })
    .await
    .unwrap();
    system.state.wait_event_loop().await;
  }
}
