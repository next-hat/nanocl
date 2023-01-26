use nanoclrs::NanoclClient;

use crate::models::{NamespaceArgs, NamespaceCommands, NamespaceOpts, NamespaceRow};

use crate::error::CliError;
use super::utils::print_table;

async fn exec_namespace_list(client: &NanoclClient) -> Result<(), CliError> {
  let items = client.list_namespace().await?;
  let namespaces = items
    .into_iter()
    .map(NamespaceRow::from)
    .collect::<Vec<NamespaceRow>>();
  print_table(namespaces);
  Ok(())
}

async fn exec_namespace_create(
  client: &NanoclClient,
  options: &NamespaceOpts,
) -> Result<(), CliError> {
  let item = client.create_namespace(&options.name).await?;
  println!("{}", item.name);
  Ok(())
}

async fn exec_namespace_inspect(
  client: &NanoclClient,
  options: &NamespaceOpts,
) -> Result<(), CliError> {
  let namespace = client.inspect_namespace(&options.name).await?;
  let namespace = serde_yaml::to_string(&namespace)?;
  println!("{}", &namespace);
  Ok(())
}

async fn exec_namespace_delete(
  client: &NanoclClient,
  options: &NamespaceOpts,
) -> Result<(), CliError> {
  client.delete_namespace(&options.name).await?;
  Ok(())
}

pub async fn exec_namespace(
  client: &NanoclClient,
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
