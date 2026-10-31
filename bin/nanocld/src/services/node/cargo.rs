use ntex::web;

use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::node::{
  DeleteNodeCargoOperationParams, DeleteNodeCargoParams,
  StartNodeCargoOperationParams, StartNodeCargoParams,
};

use crate::models::{OperationDb, SystemState};

/// Legacy route retained to reject unfenced Cargo execution explicitly.
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Nodes",
  path = "/nodes/cargoes/start",
  request_body = StartNodeCargoParams,
  responses(
    (status = 409, description = "Use the operation-aware Cargo node endpoint"),
  ),
))]
pub async fn start_node_cargo(
  _state: &SystemState,
  _request_state: (),
  _params: web::types::Json<StartNodeCargoParams>,
) -> HttpResult<web::HttpResponse> {
  Err(HttpError::conflict(
    "Cargo node execution requires /nodes/cargoes/operations/start and an accepted operation identity",
  ))
}

/// Legacy route retained to reject unfenced Cargo deletion explicitly.
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Nodes",
  path = "/nodes/cargoes",
  params(
    ("CargoKey" = String, Query, description = "Canonical cargo key in {namespace}.{name} format"),
  ),
  responses(
    (status = 409, description = "Use the operation-aware Cargo node endpoint"),
  ),
))]
pub async fn delete_node_cargo(
  _state: &SystemState,
  _request_state: (),
  _params: web::types::Query<DeleteNodeCargoParams>,
) -> HttpResult<web::HttpResponse> {
  Err(HttpError::conflict(
    "Cargo node execution requires /nodes/cargoes/operations and an accepted operation identity",
  ))
}

fn node_operation_matches(
  operation: &OperationDb,
  cargo_key: &str,
  operation_key: uuid::Uuid,
  incarnation_key: uuid::Uuid,
  spec_key: uuid::Uuid,
  action: &str,
) -> bool {
  operation.resource_kind == "Cargo"
    && operation.resource_key == cargo_key
    && operation.key == operation_key
    && operation.incarnation_key == incarnation_key
    && operation.spec_key == spec_key
    && operation.action == action
    && operation.outcome.is_none()
    && operation.completed_at.is_none()
}

async fn current_node_operation(
  cargo_key: &str,
  operation_key: uuid::Uuid,
  incarnation_key: uuid::Uuid,
  spec_key: uuid::Uuid,
  action: &str,
  state: &SystemState,
) -> HttpResult<OperationDb> {
  crate::utils::key::parse_resource_key(cargo_key)?;
  let operation = OperationDb::current("Cargo", cargo_key, &state.inner.pool)
    .await?
    .filter(|operation| {
      node_operation_matches(
        operation,
        cargo_key,
        operation_key,
        incarnation_key,
        spec_key,
        action,
      )
    })
    .ok_or_else(|| {
      HttpError::conflict(
        "Cargo node request does not identify the current pending operation",
      )
    })?;
  Ok(operation)
}

/// Execute a captured start on this node. The detached worker retains ownership
/// if the HTTP waiter disappears; success acknowledges completed local work.
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Nodes",
  path = "/nodes/cargoes/operations/start",
  request_body = StartNodeCargoOperationParams,
  responses(
    (status = 200, description = "Accepted Cargo replicas started on this node"),
    (status = 409, description = "Cargo operation is stale or mismatched"),
  ),
))]
pub async fn start_node_cargo_operation(
  state: &SystemState,
  _request_state: (),
  params: web::types::Json<StartNodeCargoOperationParams>,
) -> HttpResult<web::HttpResponse> {
  let params = params.into_inner();
  let operation = current_node_operation(
    &params.cargo_key,
    params.operation_key,
    params.incarnation_key,
    params.spec_key,
    "start",
    state,
  )
  .await?;
  crate::tasks::cargo::run_node_start(operation, params.replicas, state)
    .await
    .map_err(crate::tasks::cargo::http_error)?;
  Ok(web::HttpResponse::Ok().build())
}

#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Nodes",
  path = "/nodes/cargoes/operations",
  params(
    ("CargoKey" = String, Query, description = "Canonical Cargo key"),
    ("OperationKey" = uuid::Uuid, Query, description = "Accepted operation identity"),
    ("IncarnationKey" = uuid::Uuid, Query, description = "Accepted Cargo incarnation"),
    ("SpecKey" = uuid::Uuid, Query, description = "Accepted specification identity"),
  ),
  responses(
    (status = 200, description = "Accepted Cargo runtime deleted on this node"),
    (status = 409, description = "Cargo operation is stale or mismatched"),
  ),
))]
pub async fn delete_node_cargo_operation(
  state: &SystemState,
  _request_state: (),
  params: web::types::Query<DeleteNodeCargoOperationParams>,
) -> HttpResult<web::HttpResponse> {
  let operation = current_node_operation(
    &params.cargo_key,
    params.operation_key,
    params.incarnation_key,
    params.spec_key,
    "delete",
    state,
  )
  .await?;
  crate::tasks::cargo::run_node_delete(operation, state)
    .await
    .map_err(crate::tasks::cargo::http_error)?;
  Ok(web::HttpResponse::Ok().build())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn cargo_node_operation_rejects_stale_or_cross_resource_identity() {
    let operation = OperationDb {
      key: uuid::Uuid::from_u128(1),
      incarnation_key: uuid::Uuid::from_u128(2),
      resource_kind: "Cargo".into(),
      resource_key: "global.api".into(),
      action: "start".into(),
      spec_key: uuid::Uuid::from_u128(3),
      node_name: "coordinator".into(),
      owner_boot_id: "boot".into(),
      lock_device: 1,
      lock_inode: 2,
      accepted_at: chrono::DateTime::from_timestamp(1, 0).unwrap().naive_utc(),
      completed_at: None,
      outcome: None,
      diagnostic: None,
    };
    let accepts = |candidate: &OperationDb| {
      node_operation_matches(
        candidate,
        "global.api",
        operation.key,
        operation.incarnation_key,
        operation.spec_key,
        "start",
      )
    };
    assert!(accepts(&operation));
    for field in [
      "kind",
      "resource",
      "operation",
      "incarnation",
      "spec",
      "action",
      "outcome",
      "completed",
    ] {
      let mut stale = operation.clone();
      match field {
        "kind" => stale.resource_kind = "Vm".into(),
        "resource" => stale.resource_key = "other.api".into(),
        "operation" => stale.key = uuid::Uuid::from_u128(4),
        "incarnation" => stale.incarnation_key = uuid::Uuid::from_u128(4),
        "spec" => stale.spec_key = uuid::Uuid::from_u128(4),
        "action" => stale.action = "delete".into(),
        "outcome" => stale.outcome = Some("completed".into()),
        "completed" => stale.completed_at = Some(stale.accepted_at),
        _ => unreachable!(),
      }
      assert!(
        !accepts(&stale),
        "mismatched {field} must reject node execution"
      );
    }
  }

  #[ntex::test]
  async fn cargo_operation_delete_query_round_trips_required_identity() {
    let params = DeleteNodeCargoOperationParams {
      cargo_key: "global.api".into(),
      operation_key: uuid::Uuid::from_u128(1),
      incarnation_key: uuid::Uuid::from_u128(2),
      spec_key: uuid::Uuid::from_u128(3),
    };
    let request = ntex::client::Client::new()
      .delete("http://localhost/nodes/cargoes/operations")
      .query(&params);
    let uri = request.get_uri().to_string();
    let (request, mut payload, ()) = web::test::TestRequest::with_uri(&uri)
      .method(ntex::http::Method::DELETE)
      .to_srv_request()
      .into_parts();
    let extracted = web::test::from_request::<
      _,
      web::types::Query<DeleteNodeCargoOperationParams>,
    >(&(), &request, &mut payload)
    .await
    .unwrap();
    assert_eq!(extracted.into_inner(), params);
    for missing in ["OperationKey", "IncarnationKey", "SpecKey"] {
      let query = uri
        .split_once('?')
        .unwrap()
        .1
        .split('&')
        .filter(|field| !field.starts_with(&format!("{missing}=")))
        .collect::<Vec<_>>()
        .join("&");
      let (request, mut payload, ()) = web::test::TestRequest::with_uri(
        &format!("/nodes/cargoes/operations?{query}"),
      )
      .method(ntex::http::Method::DELETE)
      .to_srv_request()
      .into_parts();
      assert!(
        web::test::from_request::<
          _,
          web::types::Query<DeleteNodeCargoOperationParams>,
        >(&(), &request, &mut payload)
        .await
        .is_err(),
        "{missing}"
      );
    }
  }

  #[ntex::test]
  async fn delete_node_cargo_extracts_serialized_client_query_without_body() {
    for cargo_key in ["global.api", "workspace.analytics-worker"] {
      let params = DeleteNodeCargoParams {
        cargo_key: cargo_key.to_owned(),
      };
      // Exercise the same serializer as NanocldClient::send_delete without
      // sending a request or constructing database/Docker-backed state.
      let client_request = ntex::client::Client::new()
        .delete("http://localhost/nodes/cargoes")
        .query(&params);
      assert_eq!(
        client_request.get_uri().query().unwrap().as_str(),
        format!("CargoKey={cargo_key}")
      );
      let (request, mut payload, ()) =
        web::test::TestRequest::with_uri(&client_request.get_uri().to_string())
          .method(ntex::http::Method::DELETE)
          .to_srv_request()
          .into_parts();

      let extracted = web::test::from_request::<
        _,
        web::types::Query<DeleteNodeCargoParams>,
      >(&(), &request, &mut payload)
      .await
      .unwrap();
      assert_eq!(extracted.cargo_key, cargo_key);
    }
  }

  #[ntex::test]
  async fn delete_node_cargo_query_rejects_missing_or_ambiguous_cargo_key() {
    for query in [
      "",
      "cargo_key=global.api",
      "CargoKey=global.api&CargoKey=global.worker",
    ] {
      let (request, mut payload, ()) =
        web::test::TestRequest::with_uri(&format!("/nodes/cargoes?{query}"))
          .method(ntex::http::Method::DELETE)
          .json(&DeleteNodeCargoParams {
            cargo_key: "global.body-must-not-select-cargo".to_owned(),
          })
          .to_srv_request()
          .into_parts();

      assert!(
        web::test::from_request::<_, web::types::Query<DeleteNodeCargoParams>>(
          &(),
          &request,
          &mut payload,
        )
        .await
        .is_err(),
        "invalid query must not fall back to the JSON body: {query}"
      );
    }
  }
}
