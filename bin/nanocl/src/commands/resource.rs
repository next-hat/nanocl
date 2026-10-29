use nanocl_error::io::{FromIo, IoResult};
use nanocld_client::stubs::resource::Resource;

use crate::{
  config::CliConfig,
  models::{
    DisplayFormat, GenericDefaultOpts, GenericHistoryOpts, ResourceArg,
    ResourceCommand, ResourceCompactRow, ResourceRevertOpts, ResourceRow,
    SpecHistoryRow,
  },
  utils,
};

use super::{
  GenericCommand, GenericCommandInspect, GenericCommandLs, GenericCommandRm,
};

impl GenericCommand for ResourceArg {
  fn object_name() -> &'static str {
    "resources"
  }
}

impl GenericCommandLs for ResourceArg {
  type Item = ResourceRow;
  type CompactItem = ResourceCompactRow;
  type Args = ResourceArg;
  type ApiItem = nanocld_client::stubs::resource::Resource;

  fn get_key(item: &Self::Item) -> String {
    item.name.clone()
  }
}

impl GenericCommandRm<GenericDefaultOpts, String> for ResourceArg {}

impl GenericCommandInspect for ResourceArg {
  type ApiItem = Resource;
}

/// Function that execute when running `nanocl resource history`
async fn exec_resource_history(
  cli_conf: &CliConfig,
  opts: &GenericHistoryOpts,
) -> IoResult<()> {
  super::history::validate_options(opts)?;
  let history = cli_conf.client.list_history_resource(&opts.key).await?;
  super::history::print_revision_history(
    opts,
    &cli_conf.user_config.display_format,
    history,
    |item| item.key,
    SpecHistoryRow::from,
  )
}

/// Function that execute when running `nanocl resource revert`
async fn exec_resource_revert(
  cli_conf: &CliConfig,
  opts: &ResourceRevertOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let resource = client.revert_resource(&opts.name, &opts.key).await?;
  print_reverted_resource(&cli_conf.user_config.display_format, resource)
}

fn print_reverted_resource(
  format: &DisplayFormat,
  resource: Resource,
) -> IoResult<()> {
  utils::print::display_format(format, &resource).map_err(|error| {
    let name = &resource.spec.resource_key;
    let context = format!(
      "Resource revert succeeded, but configured {format} output cannot represent the response; do not repeat the revert. Inspect with `nanocl resource inspect {name} --display json` or `nanocl resource inspect {name} --display yaml` ({})",
      error.context().unwrap_or("Output"),
    );
    error.map_err_context(|| context)
  })
}

/// Function that execute when running `nanocl resource`
pub async fn exec_resource(
  cli_conf: &CliConfig,
  args: &ResourceArg,
) -> IoResult<()> {
  match &args.command {
    ResourceCommand::List(opts) => {
      ResourceArg::exec_ls(&cli_conf.client, args, opts, None).await
    }
    ResourceCommand::Remove(opts) => {
      ResourceArg::exec_rm(&cli_conf.client, opts).await
    }
    ResourceCommand::Inspect(opts) => {
      ResourceArg::exec_inspect(cli_conf, opts).await
    }
    ResourceCommand::History(opts) => {
      exec_resource_history(cli_conf, opts).await
    }
    ResourceCommand::Revert(opts) => exec_resource_revert(cli_conf, opts).await,
  }
}

#[cfg(test)]
mod tests {
  use nanocld_client::stubs::resource::ResourceSpec;

  use super::*;

  fn reverted_resource(data: serde_json::Value) -> Resource {
    let created_at = chrono::DateTime::UNIX_EPOCH.naive_utc();
    Resource {
      kind: "test".into(),
      created_at,
      spec: ResourceSpec {
        key: uuid::Uuid::from_u128(1),
        version: "v0.19".into(),
        created_at,
        resource_key: "test-resource".into(),
        data,
        metadata: None,
      },
    }
  }

  #[test]
  fn resource_revert_output_reports_success_for_unrepresentable_toml() {
    for data in [
      serde_json::Value::Null,
      serde_json::json!({"nested": {"value": null}}),
    ] {
      let resource = reverted_resource(data);
      let cause = toml::to_string(&resource).unwrap_err().to_string();
      let error =
        print_reverted_resource(&DisplayFormat::Toml, resource.clone())
          .unwrap_err();
      assert_eq!(error.inner.kind(), std::io::ErrorKind::InvalidData);
      assert_eq!(error.inner.to_string(), cause);
      let message = error.to_string();
      for expected in [
        "Resource revert succeeded",
        "configured toml output cannot represent the response",
        "do not repeat the revert",
        "nanocl resource inspect test-resource --display json",
        "nanocl resource inspect test-resource --display yaml",
        "Print toml",
      ] {
        assert!(message.contains(expected), "{message}");
      }
      print_reverted_resource(&DisplayFormat::Json, resource.clone()).unwrap();
      print_reverted_resource(&DisplayFormat::Yaml, resource).unwrap();
    }
  }

  #[test]
  fn resource_revert_output_supports_representable_toml() {
    let resource =
      reverted_resource(serde_json::json!({"nested": {"value": "present"}}));
    print_reverted_resource(&DisplayFormat::Toml, resource).unwrap();
  }
}
