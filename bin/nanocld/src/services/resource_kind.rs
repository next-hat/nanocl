use ntex::web;

use nanocl_error::http::HttpResult;

use nanocl_stubs::{
  generic::GenericFilter,
  resource_kind::{ResourceKindPartial, ResourceKindVersion},
};

use crate::{
  repositories::generic::*,
  models::{DaemonState, ResourceKindDb, SpecDb},
};

/// List resource kinds
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "ResourceKinds",
  path = "/resource/kinds",
  responses(
    (status = 200, description = "List of jobs", body = [ResourceKind]),
  ),
))]
#[web::get("/resource/kinds")]
pub(crate) async fn list_resource_kind(
  state: web::types::State<DaemonState>,
  _version: web::types::Path<String>,
) -> HttpResult<web::HttpResponse> {
  let filter = GenericFilter::new();
  let resource_kinds =
    ResourceKindDb::read_with_spec(&filter, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&resource_kinds))
}

/// Create a resource kind
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  tag = "ResourceKinds",
  path = "/resource/kinds",
  request_body = ResourceKindPartial,
  responses(
    (status = 201, description = "Job created", body = ResourceKind),
  ),
))]
#[web::post("/resource/kinds")]
pub(crate) async fn create_resource_kind(
  state: web::types::State<DaemonState>,
  _version: web::types::Path<String>,
  payload: web::types::Json<ResourceKindPartial>,
) -> HttpResult<web::HttpResponse> {
  let item = ResourceKindDb::create_from_spec(&payload, &state.pool).await?;
  Ok(web::HttpResponse::Created().json(&item))
}

/// Delete a resource kind
#[cfg_attr(feature = "dev", utoipa::path(
  delete,
  tag = "ResourceKinds",
  path = "/resource/kinds/{domain}/{name}",
  params(
    ("domain" = String, Path, description = "Domain of the resource kind"),
    ("name" = String, Path, description = "Name of the resource kind"),
  ),
  responses(
    (status = 202, description = "Resource kind deleted"),
    (status = 404, description = "Resource kind does not exist"),
  ),
))]
#[web::delete("/resource/kinds/{domain}/{name}")]
pub(crate) async fn delete_resource_kind(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String, String)>,
) -> HttpResult<web::HttpResponse> {
  let key = format!("{}/{}", path.1, path.2);
  ResourceKindDb::read_pk_with_spec(&key, &state.pool).await?;
  ResourceKindDb::del_by_pk(&key, &state.pool).await?;
  SpecDb::del_by_kind_key(&key, &state.pool).await?;
  Ok(web::HttpResponse::Accepted().into())
}

/// Inspect a resource kind
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "ResourceKinds",
  path = "/resource/kinds/{domain}/{name}/inspect",
  params(
    ("domain" = String, Path, description = "Domain of the resource kind"),
    ("name" = String, Path, description = "Name of the resource kind"),
  ),
  responses(
    (status = 200, description = "Details about a resource kind", body = ResourceKindInspect),
  ),
))]
#[web::get("/resource/kinds/{domain}/{name}/inspect")]
pub(crate) async fn inspect_resource_kind(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String, String)>,
) -> HttpResult<web::HttpResponse> {
  let key: String = format!("{}/{}", path.1, path.2);
  let kind = ResourceKindDb::inspect_by_pk(&key, &state.pool).await?;
  Ok(web::HttpResponse::Ok().json(&kind))
}

/// Inspect a specific version of a resource kind
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  tag = "ResourceKinds",
  path = "/resource/kinds/{domain}/{name}/version/{version}",
  params(
    ("domain" = String, Path, description = "Domain of the resource kind"),
    ("name" = String, Path, description = "Name of the resource kind"),
  ),
  responses(
    (status = 200, description = "Details about a resource kind", body = ResourceKindVersion),
  ),
))]
#[web::get("/resource/kinds/{domain}/{name}/version/{version}/inspect")]
pub(crate) async fn inspect_resource_kind_version(
  state: web::types::State<DaemonState>,
  path: web::types::Path<(String, String, String, String)>,
) -> HttpResult<web::HttpResponse> {
  let key = format!("{}/{}", path.1, path.2);
  let kind_version = SpecDb::get_version(&key, &path.3, &state.pool).await?;
  let kind_version: ResourceKindVersion = kind_version.try_into()?;
  Ok(web::HttpResponse::Ok().json(&kind_version))
}

pub(crate) fn ntex_config(config: &mut web::ServiceConfig) {
  config
    .service(list_resource_kind)
    .service(create_resource_kind)
    .service(delete_resource_kind)
    .service(inspect_resource_kind)
    .service(inspect_resource_kind_version);
}

#[cfg(test)]
mod tests {
  use ntex::http;

  const ENDPOINT: &str = "/resource/kinds";

  use crate::utils::tests::*;

  use nanocl_stubs::resource_kind::{
    ResourceKind, ResourceKindPartial, ResourceKindSpec, ResourceKindInspect,
    ResourceKindVersion,
  };

  #[ntex::test]
  async fn test_inspect_version_not_found() {
    let client = gen_default_test_client().await;
    let res = client
      .send_get(
        &format!("{}/test.io/api-test/version/v12/inspect", ENDPOINT),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::NOT_FOUND,
      "resource kind inspect version"
    );
  }

  #[ntex::test]
  async fn test_wrong_name() {
    let client = gen_default_test_client().await;
    let payload = ResourceKindPartial {
      name: "api-test".to_owned(),
      version: "v1".to_owned(),
      metadata: None,
      data: ResourceKindSpec {
        schema: None,
        url: Some("unix:///run/nanocl/proxy.sock".to_owned()),
      },
    };
    let res = client
      .send_post(ENDPOINT, Some(&payload), None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::BAD_REQUEST,
      "resource kind create"
    );
  }

  #[ntex::test]
  async fn test_wrong_spec() {
    let client = gen_default_test_client().await;
    let payload = ResourceKindPartial {
      name: "test.io/api-test".to_owned(),
      version: "v1".to_owned(),
      metadata: None,
      data: ResourceKindSpec {
        schema: None,
        url: None,
      },
    };
    let res = client
      .send_post(ENDPOINT, Some(&payload), None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::BAD_REQUEST,
      "resource kind create"
    );
  }
  #[ntex::test]
  async fn basic_list() {
    let client = gen_default_test_client().await;
    // Create
    let payload = ResourceKindPartial {
      name: "test.io/api-test".to_owned(),
      version: "v1".to_owned(),
      metadata: None,
      data: ResourceKindSpec {
        schema: None,
        url: Some("unix:///run/nanocl/proxy.sock".to_owned()),
      },
    };
    let mut res = client
      .send_post(ENDPOINT, Some(&payload), None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::CREATED,
      "resource kind create"
    );
    let kind = res.json::<ResourceKind>().await.unwrap();
    assert_eq!(kind.name, payload.name);
    assert_eq!(kind.version, payload.version);
    // List
    let mut res = client.send_get(ENDPOINT, None::<String>).await;
    test_status_code!(res.status(), http::StatusCode::OK, "resource kind list");
    let items = res.json::<Vec<ResourceKind>>().await.unwrap();
    assert!(items.iter().any(|i| i.name == payload.name));
    // Inspect
    let mut res = client
      .send_get(
        &format!("{}/{}/inspect", ENDPOINT, payload.name),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "resource kind inspect"
    );
    let kind = res.json::<ResourceKindInspect>().await.unwrap();
    assert_eq!(kind.name, payload.name);
    // Inspect version
    let mut res = client
      .send_get(
        &format!(
          "{}/{}/version/{}/inspect",
          ENDPOINT, payload.name, payload.version
        ),
        None::<String>,
      )
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::OK,
      "resource kind inspect version"
    );
    let _ = res.json::<ResourceKindVersion>().await.unwrap();
    // Delete
    let res = client
      .send_delete(&format!("{}/{}", ENDPOINT, payload.name), None::<String>)
      .await;
    test_status_code!(
      res.status(),
      http::StatusCode::ACCEPTED,
      "resource kind delete"
    );
  }
}
