use nanocl_error::io::{IoResult, FromIo};
use nanocld_client::NanocldClient;

use crate::config::CliConfig;
use crate::utils;
use crate::models::{
  NamespaceArg, NamespaceCommand, NamespaceOpts, NamespaceRow,
  NamespaceDeleteOpts, NamespaceListOpts,
};

/// Function that execute when running `nanocl namespace ls`
async fn exec_namespace_ls(
  client: &NanocldClient,
  options: &NamespaceListOpts,
) -> IoResult<()> {
  let items = client.list_namespace().await?;
  let namespaces = items
    .into_iter()
    .map(NamespaceRow::from)
    .collect::<Vec<NamespaceRow>>();
  match options.quiet {
    true => {
      for namespace in namespaces {
        println!("{}", namespace.name);
      }
    }
    false => {
      utils::print::print_table(namespaces);
    }
  }
  Ok(())
}

/// Function that execute when running `nanocl namespace create`
async fn exec_namespace_create(
  client: &NanocldClient,
  options: &NamespaceOpts,
) -> IoResult<()> {
  let item = client.create_namespace(&options.name).await?;
  println!("{}", item.name);
  Ok(())
}

/// Function that execute when running `nanocl namespace inspect`
async fn exec_namespace_inspect(
  client: &NanocldClient,
  options: &NamespaceOpts,
) -> IoResult<()> {
  let namespace = client.inspect_namespace(&options.name).await?;
  utils::print::print_yml(namespace)?;
  Ok(())
}

/// Function that execute when running `nanocl namespace rm`
async fn exec_namespace_rm(
  client: &NanocldClient,
  options: &NamespaceDeleteOpts,
) -> IoResult<()> {
  if !options.skip_confirm {
    utils::dialog::confirm(&format!(
      "Delete namespace {}?",
      options.names.join(",")
    ))
    .map_err(|err| err.map_err_context(|| "Delete namespace"))?;
  }
  for name in &options.names {
    client.delete_namespace(name).await?;
  }
  Ok(())
}

/// Function that execute when running `nanocl namespace`
pub async fn exec_namespace(
  cli_conf: &CliConfig,
  args: &NamespaceArg,
) -> IoResult<()> {
  let client = &cli_conf.client;
  match &args.command {
    NamespaceCommand::List(options) => exec_namespace_ls(client, options).await,
    NamespaceCommand::Create(options) => {
      exec_namespace_create(client, options).await
    }
    NamespaceCommand::Inspect(options) => {
      exec_namespace_inspect(client, options).await
    }
    NamespaceCommand::Remove(options) => {
      exec_namespace_rm(client, options).await
    }
  }
}
