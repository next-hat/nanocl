use dialoguer::Confirm;
use nanocld_client::NanocldClient;

use crate::utils::print::*;
use crate::error::CliError;
use crate::models::{
  NamespaceArgs, NamespaceCommands, NamespaceOpts, NamespaceRow,
  NamespaceDeleteOpts,
};

async fn exec_namespace_list(client: &NanocldClient) -> Result<(), CliError> {
  let items = client.list_namespace().await?;
  let namespaces = items
    .into_iter()
    .map(NamespaceRow::from)
    .collect::<Vec<NamespaceRow>>();
  print_table(namespaces);
  Ok(())
}

async fn exec_namespace_create(
  client: &NanocldClient,
  options: &NamespaceOpts,
) -> Result<(), CliError> {
  let item = client.create_namespace(&options.name).await?;
  println!("{}", item.name);
  Ok(())
}

async fn exec_namespace_inspect(
  client: &NanocldClient,
  options: &NamespaceOpts,
) -> Result<(), CliError> {
  let namespace = client.inspect_namespace(&options.name).await?;
  print_yml(namespace)?;
  Ok(())
}

async fn exec_namespace_delete(
  client: &NanocldClient,
  options: &NamespaceDeleteOpts,
) -> Result<(), CliError> {
  if !options.skip_confirm {
    let result = Confirm::new()
      .with_prompt(format!("Delete namespace {}?", options.name))
      .default(false)
      .interact();
    match result {
      Ok(true) => {}
      _ => {
        return Err(CliError::Custom {
          msg: "Aborted".into(),
        })
      }
    }
  }

  client.delete_namespace(&options.name).await?;
  Ok(())
}

pub async fn exec_namespace(
  client: &NanocldClient,
  args: &NamespaceArgs,
) -> Result<(), CliError> {
  match &args.commands {
    NamespaceCommands::List => exec_namespace_list(client).await,
    NamespaceCommands::Create(options) => {
      exec_namespace_create(client, options).await
    }
    NamespaceCommands::Inspect(options) => {
      exec_namespace_inspect(client, options).await
    }
    NamespaceCommands::Remove(options) => {
      exec_namespace_delete(client, options).await
    }
  }
}
