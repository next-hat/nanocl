use nanocl_error::io::IoResult;
use nanocld_client::{stubs::namespace::NamespaceInspect, NanocldClient};

use crate::{
  config::CliConfig,
  models::{
    GenericDefaultOpts, NamespaceArg, NamespaceCommand, NamespaceCreateOpts,
    NamespaceRow,
  },
};
use nanocld_client::stubs::namespace::NamespaceSummary;

use super::{
  GenericCommand, GenericCommandInspect, GenericCommandLs, GenericCommandRm,
};

impl GenericCommand for NamespaceArg {
  fn object_name() -> &'static str {
    "namespaces"
  }
}

impl GenericCommandLs for NamespaceArg {
  type Item = NamespaceRow;
  type Args = NamespaceArg;
  type ApiItem = NamespaceSummary;

  fn get_key(item: &Self::Item) -> String {
    item.name.clone()
  }
}

impl GenericCommandRm<GenericDefaultOpts, String> for NamespaceArg {}

impl GenericCommandInspect for NamespaceArg {
  type ApiItem = NamespaceInspect;
}

/// Function that execute when running `nanocl namespace create`
async fn exec_namespace_create(
  client: &NanocldClient,
  opts: &NamespaceCreateOpts,
) -> IoResult<()> {
  let item = client.create_namespace(&opts.name).await?;
  println!("{}", item.name);
  Ok(())
}

/// Function that execute when running `nanocl namespace`
pub async fn exec_namespace(
  cli_conf: &CliConfig,
  args: &NamespaceArg,
) -> IoResult<()> {
  let client = &cli_conf.client;
  match &args.command {
    NamespaceCommand::List(opts) => {
      NamespaceArg::exec_ls(client, args, opts).await
    }
    NamespaceCommand::Create(opts) => exec_namespace_create(client, opts).await,
    NamespaceCommand::Inspect(opts) => {
      NamespaceArg::exec_inspect(cli_conf, opts, None).await
    }
    NamespaceCommand::Remove(opts) => {
      NamespaceArg::exec_rm(client, opts, None).await
    }
  }
}
