use ntex::web;

use nanocl_error::http::HttpResult;
use nanocl_stubs::node::{DeleteNodeCargoParams, StartNodeCargoParams};

use crate::{
  models::{CargoDb, SystemState},
  repositories::generic::*,
  utils,
};

/// Start cargo instances on the node used for node to node communication
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "Nodes",
  path = "/nodes/cargoes/start",
  request_body = StartNodeCargoParams,
  responses(
    (status = 200, description = "Cargoes sucessfully started"),
  ),
))]
pub async fn start_node_cargo(
  state: &SystemState,
  _request_state: (),
  params: web::types::Json<StartNodeCargoParams>,
) -> HttpResult<web::HttpResponse> {
  let cargo =
    CargoDb::transform_read_by_pk(&params.cargo_key, &state.inner.pool).await?;
  utils::container::cargo::start(&cargo, &params.replicas, &state).await?;
  Ok(web::HttpResponse::Ok().build())
}

/// Delete cargo instances on the node used for node to node communication
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "Nodes",
  path = "/nodes/cargoes",
  params(
    ("CargoKey" = String, Query, description = "Canonical cargo key in {namespace}.{name} format"),
  ),
  responses(
    (status = 200, description = "Cargoes sucessfully deleted"),
  ),
))]
pub async fn delete_node_cargo(
  state: &SystemState,
  _request_state: (),
  params: web::types::Query<DeleteNodeCargoParams>,
) -> HttpResult<web::HttpResponse> {
  utils::container::cargo::delete_instances(&params.cargo_key, false, &state)
    .await?;
  Ok(web::HttpResponse::Ok().build())
}

#[cfg(test)]
mod tests {
  use super::*;

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
