use utoipa::OpenApi;

use nanocld_client::stubs::proxy::{
  ProxyRule, ProxyRuleHttp, ProxyRuleStream, ResourceProxyRule,
  ProxyHttpLocation, ProxySslConfig, ProxyStreamProtocol, StreamTarget,
  LocationTarget, CargoTarget, HttpTarget, UriTarget, UrlRedirect,
};

use crate::service;

/// Main structure to generate OpenAPI documentation
#[derive(OpenApi)]
#[openapi(
  paths(
    service::apply_rule,
    service::remove_rule,
  ),
  components(schemas(
    ResourceProxyRule,
    ProxyRule,
    ProxyRuleHttp,
    ProxyRuleStream,
    ProxyHttpLocation,
    ProxySslConfig,
    ProxyStreamProtocol,
    StreamTarget,
    LocationTarget,
    CargoTarget,
    HttpTarget,
    UriTarget,
    UrlRedirect,
  )),
  tags(
    (name = "Rules", description = "Rules management endpoints."),
  ),
)]
pub(crate) struct ApiDoc;
