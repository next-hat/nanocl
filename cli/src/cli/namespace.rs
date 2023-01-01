use futures::StreamExt;
use futures::stream::FuturesUnordered;

use crate::client::Nanocld;
use crate::models::{
  NamespaceArgs, NamespaceCommands, NamespaceWithCount, NamespacePartial,
};

use super::errors::CliError;
use super::utils::print_table;

async fn exec_namespace_list(client: &Nanocld) -> Result<(), CliError> {
  let items = client.list_namespace().await?;
  let namespaces = items
    .iter()
    .map(|item| async {
      let new_item = NamespaceWithCount {
        name: item.name.to_owned(),
        cargoes: 1,
        clusters: 1,
        networks: 1,
      };
      Ok::<_, CliError>(new_item)
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<NamespaceWithCount>, CliError>>()?;
  print_table(namespaces);
  Ok(())
}

async fn exec_namespace_create(
  client: &Nanocld,
  item: &NamespacePartial,
) -> Result<(), CliError> {
  let item = client.create_namespace(&item.name).await?;
  println!("{}", item.name);
  Ok(())
}

pub async fn exec_namespace(
  client: &Nanocld,
  args: &NamespaceArgs,
) -> Result<(), CliError> {
  match &args.commands {
    NamespaceCommands::List => exec_namespace_list(client).await,
    NamespaceCommands::Create(item) => {
      exec_namespace_create(client, item).await
    }
  }
}
