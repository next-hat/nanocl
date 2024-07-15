use utoipa::OpenApi;

use nanocld_client::stubs::dns::{DnsEntry, ResourceDnsRule};

use super::rule;

/// Helper to generate the versioned OpenAPI documentation
struct VersionModifier;

impl utoipa::Modify for VersionModifier {
  fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
    let variable = utoipa::openapi::ServerVariableBuilder::default()
      .default_value("v0.6")
      .description(Some("API version"))
      .enum_values(Some(vec!["v0.5"]))
      .build();
    let server = utoipa::openapi::ServerBuilder::default()
      .url("/{Version}")
      .parameter("Version", variable)
      .build();
    "Nanocl Controller Dns".clone_into(&mut openapi.info.title);
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
    ResourceDnsRule,
    DnsEntry,
  )),
  tags(
    (name = "Rules", description = "Rules management endpoints."),
  ),
  modifiers(&VersionModifier),
)]
pub(crate) struct ApiDoc;
