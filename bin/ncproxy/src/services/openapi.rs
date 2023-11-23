use utoipa::OpenApi;

use nanocld_client::stubs::proxy::{
  ProxyRule, ProxyRuleHttp, ProxyRuleStream, ResourceProxyRule,
  ProxyHttpLocation, ProxySsl, ProxyStreamProtocol, StreamTarget,
  LocationTarget, UpstreamTarget, HttpTarget, UriTarget, UrlRedirect,
  UnixTarget,
};

use super::rule;

/// Helper to generate the versioned OpenAPI documentation
struct VersionModifier;

impl utoipa::Modify for VersionModifier {
  fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
    let variable = utoipa::openapi::ServerVariableBuilder::default()
      .default_value("v0.8")
      .description(Some("API version"))
      .enum_values(Some(vec![
        "v0.8", "v0.7", "v0.6", "v0.5", "v0.4", "v0.3", "v0.2", "v0.1",
      ]))
      .build();
    let server = utoipa::openapi::ServerBuilder::default()
      .url("/{Version}")
      .parameter("Version", variable)
      .build();
    openapi.info.title = "Nanocl Controller Proxy".to_owned();
    openapi.info.version = format!("v{}", env!("CARGO_PKG_VERSION"));
    openapi.info.description =
      Some(include_str!("../../specs/readme.md").to_owned());
    openapi.servers = Some(vec![server]);
  }
}

/// Main structure to generate OpenAPI documentation
#[derive(OpenApi)]
#[openapi(
  paths(
    rule::apply_rule,
    rule::remove_rule,
  ),
  components(schemas(
    ResourceProxyRule,
    ProxyRule,
    ProxyRuleHttp,
    ProxyRuleStream,
    ProxyHttpLocation,
    ProxySsl,
    ProxyStreamProtocol,
    StreamTarget,
    LocationTarget,
    UpstreamTarget,
    HttpTarget,
    UriTarget,
    UrlRedirect,
    UnixTarget,
  )),
  tags(
    (name = "Rules", description = "Rules management endpoints."),
  ),
  modifiers(&VersionModifier),
)]
pub(crate) struct ApiDoc;
