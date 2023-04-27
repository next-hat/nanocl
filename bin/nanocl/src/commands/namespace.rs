use dialoguer::Confirm;
use dialoguer::theme::ColorfulTheme;
use nanocld_client::NanocldClient;

use nanocl_utils::io_error::{IoError, IoResult};

use crate::utils::print::{print_yml, print_table};
use crate::models::{
  NamespaceArgs, NamespaceCommands, NamespaceOpts, NamespaceRow,
  NamespaceDeleteOpts,
};

async fn exec_namespace_ls(client: &NanocldClient) -> IoResult<()> {
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
) -> IoResult<()> {
  let item = client.create_namespace(&options.name).await?;
  println!("{}", item.name);
  Ok(())
}

async fn exec_namespace_inspect(
  client: &NanocldClient,
  options: &NamespaceOpts,
) -> IoResult<()> {
  let namespace = client.inspect_namespace(&options.name).await?;
  print_yml(namespace)?;
  Ok(())
}

async fn exec_namespace_rm(
  client: &NanocldClient,
  options: &NamespaceDeleteOpts,
) -> IoResult<()> {
  if !options.skip_confirm {
    let result = Confirm::with_theme(&ColorfulTheme::default())
      .with_prompt(format!("Delete namespaces {}?", options.names.join(",")))
      .default(false)
      .interact();
    match result {
      Ok(true) => {}
      _ => {
        return Err(IoError::interupted(
          "Namespace remove",
          "interupted by user",
        ))
      }
    }
  }

  for name in &options.names {
    client.delete_namespace(name).await?;
  }

  Ok(())
}

pub async fn exec_namespace(
  client: &NanocldClient,
  args: &NamespaceArgs,
) -> IoResult<()> {
  match &args.commands {
    NamespaceCommands::List => exec_namespace_ls(client).await,
    NamespaceCommands::Create(options) => {
      exec_namespace_create(client, options).await
    }
    NamespaceCommands::Inspect(options) => {
      exec_namespace_inspect(client, options).await
    }
    NamespaceCommands::Remove(options) => {
      exec_namespace_rm(client, options).await
    }
  }
}
