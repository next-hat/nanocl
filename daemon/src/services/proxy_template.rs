use ntex::web;

use crate::repositories;
use crate::models::{Pool, ProxyTemplateItem};

use crate::errors::HttpResponseError;

/// List all proxy template
#[cfg_attr(feature = "dev", utoipa::path(
  get,
  path = "/proxy/templates",
  responses(
      (status = 200, description = "Array of proxy templates", body = [ProxyTemplateItem]),
  ),
))]
#[web::get("/proxy/templates")]
async fn list_proxy_template(
  pool: web::types::State<Pool>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let items = repositories::proxy_template::list(&pool).await?;

  Ok(web::HttpResponse::Ok().json(&items))
}

/// Create proxy template
#[cfg_attr(feature = "dev", utoipa::path(
  post,
  path = "/proxy/templates",
  responses(
    (status = 201, description = "The new proxy Template created", body = ProxyTemplateItem)
  )
))]
#[web::post("/proxy/templates")]
async fn create_proxy_template(
  pool: web::types::State<Pool>,
  web::types::Json(payload): web::types::Json<ProxyTemplateItem>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let res = repositories::proxy_template::create(payload, &pool).await?;

  Ok(web::HttpResponse::Created().json(&res))
}

/// Delete proxy template by name
#[web::delete("/proxy/templates/{name}")]
async fn delete_proxy_template_by_name(
  pool: web::types::State<Pool>,
  name: web::types::Path<String>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let res =
    repositories::proxy_template::delete_by_name(name.into_inner(), &pool)
      .await?;

  Ok(web::HttpResponse::Ok().json(&res))
}

/// Inspect proxy template by name
#[web::get("/proxy/templates/{name}")]
async fn inspect_proxy_template_by_name(
  pool: web::types::State<Pool>,
  name: web::types::Path<String>,
) -> Result<web::HttpResponse, HttpResponseError> {
  let res =
    repositories::proxy_template::get_by_name(name.into_inner(), &pool).await?;

  Ok(web::HttpResponse::Ok().json(&res))
}

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_proxy_template);
  config.service(create_proxy_template);
  config.service(delete_proxy_template_by_name);
  config.service(inspect_proxy_template_by_name);
}

/// Proxy template unit tests
#[cfg(test)]
pub mod tests {

  use super::*;

  use ntex::http::StatusCode;

  use crate::models::{ProxyTemplateModes, GenericDelete};
  use crate::utils::tests::*;

  /// Test utils to list proxy template
  pub async fn list(srv: &TestServer) -> TestReqRet {
    srv.get("/proxy/templates").send().await
  }

  /// Test utils to create proxy template
  pub async fn create(
    srv: &TestServer,
    payload: &ProxyTemplateItem,
  ) -> TestReqRet {
    srv.post("/proxy/templates").send_json(&payload).await
  }

  /// Test utils to inspect proxy template by name
  pub async fn inspect(srv: &TestServer, name: &str) -> TestReqRet {
    srv.get(format!("/proxy/templates/{}", name)).send().await
  }

  /// Test utils to delete proxy template by name
  pub async fn delete(srv: &TestServer, name: &str) -> TestReqRet {
    srv
      .delete(format!("/proxy/templates/{}", name))
      .send()
      .await
  }

  /// Basic test to list proxy template that return StatusCode::Ok
  #[ntex::test]
  async fn basic_list() -> TestRet {
    let srv = generate_server(ntex_config).await;

    let mut res = list(&srv).await?;
    let body: serde_json::Value = res.json().await?;
    println!("proxy template list body: {:?}", body);
    assert_eq!(
      res.status(),
      StatusCode::OK,
      "Expect basic list to return {} got {}",
      StatusCode::OK,
      res.status()
    );

    Ok(())
  }

  /// Basic test that create, inspect and delete a proxy template
  #[ntex::test]
  async fn basic_create_inspect_delete() -> TestRet {
    let srv = generate_server(ntex_config).await;

    // Create
    let payload = ProxyTemplateItem {
      name: String::from("unit-test-template"),
      content: String::from("test"),
      mode: ProxyTemplateModes::Http,
    };
    let mut res = create(&srv, &payload).await?;
    let status = res.status();
    let body: serde_json::Value = res.json().await?;
    println!("body: {:?}", body);
    assert_eq!(
      status,
      StatusCode::CREATED,
      "Expected creating a proxy template with status {} got {}",
      StatusCode::CREATED,
      status
    );

    // Inspect
    let mut res = inspect(&srv, &payload.name).await?;
    let status = res.status();
    assert_eq!(
      status,
      StatusCode::OK,
      "Expected inspecting a proxy template with status {} got {}",
      StatusCode::OK,
      status
    );
    let body: ProxyTemplateItem = res
      .json()
      .await
      .expect("Expect to parse a proxy template item");
    assert_eq!(
      body.name, payload.name,
      "Expected proxy template name to be {} got {}",
      payload.name, body.name
    );

    // Delete
    let mut res = delete(&srv, &payload.name).await?;
    let status = res.status();
    assert_eq!(
      status,
      StatusCode::OK,
      "Expected deleting a proxy template with status {} got {}",
      StatusCode::OK,
      status
    );
    let body: GenericDelete =
      res.json().await.expect("Expect to parse a generic delete");
    assert_eq!(
      body.count, 1,
      "Expected deleted count to be 1 got {}",
      body.count
    );

    Ok(())
  }
}
