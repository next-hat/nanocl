use nanocld_client::NanocldClient;
use nanocl_error::io::IoResult;

use nanocld_client::stubs::namespace::NamespaceSummary;
use crate::{
  utils,
  config::CliConfig,
  models::{
    GenericDefaultOpts, NamespaceArg, NamespaceCommand, NamespaceOpts,
    NamespaceRow,
  },
};

use super::{GenericList, GenericRemove};

impl GenericList for NamespaceArg {
  type Item = NamespaceRow;
  type Args = NamespaceArg;
  type ApiItem = NamespaceSummary;

  fn object_name() -> &'static str {
    "namespaces"
  }

  fn get_key(item: &Self::Item) -> String {
    item.name.clone()
  }
}

impl GenericRemove<GenericDefaultOpts, String> for NamespaceArg {
  fn object_name() -> &'static str {
    "namespaces"
  }
}

/// Function that execute when running `nanocl namespace create`
async fn exec_namespace_create(
  client: &NanocldClient,
  opts: &NamespaceOpts,
) -> IoResult<()> {
  let item = client.create_namespace(&opts.name).await?;
  println!("{}", item.name);
  Ok(())
}

/// Function that execute when running `nanocl namespace inspect`
async fn exec_namespace_inspect(
  client: &NanocldClient,
  opts: &NamespaceOpts,
) -> IoResult<()> {
  let namespace = client.inspect_namespace(&opts.name).await?;
  utils::print::print_yml(namespace)?;
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
      exec_namespace_inspect(client, opts).await
    }
    NamespaceCommand::Remove(opts) => NamespaceArg::exec_rm(client, opts).await,
  }
}
