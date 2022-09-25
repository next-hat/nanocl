use crate::client::Nanocld;
use crate::models::{
  CargoPatchPartial, CargoArgs, CargoCommands, CargoPartial,
  CargoDeleteOptions, CargoInspectOption, CargoPatchArgs, CargoPatchCommands,
};

use super::utils::print_table;
use super::errors::CliError;

async fn exec_cargo_list(
  client: &Nanocld,
  args: &CargoArgs,
) -> Result<(), CliError> {
  let items = client.list_cargo(args.namespace.to_owned()).await?;
  print_table(items);
  Ok(())
}

async fn exec_cargo_create(
  client: &Nanocld,
  args: &CargoArgs,
  item: &CargoPartial,
) -> Result<(), CliError> {
  let item = client.create_cargo(item, args.namespace.to_owned()).await?;
  println!("{}", item.key);
  Ok(())
}

async fn exec_cargo_remove(
  client: &Nanocld,
  args: &CargoArgs,
  options: &CargoDeleteOptions,
) -> Result<(), CliError> {
  client
    .delete_cargo(&options.name, args.namespace.to_owned())
    .await?;
  Ok(())
}

async fn exec_cargo_inspect(
  client: &Nanocld,
  args: &CargoArgs,
  options: &CargoInspectOption,
) -> Result<(), CliError> {
  let cargo = client
    .inspect_cargo(&options.name, args.namespace.to_owned())
    .await?;

  println!("\n> CARGO");
  print_table([&cargo]);
  if let Some(environnements) = cargo.environnements {
    println!("\n> ENVIRONNEMENTS");
    print_table(environnements);
  }
  println!("\n> CONTAINERS");
  print_table(cargo.containers);
  Ok(())
}

async fn exec_cargo_patch_set(
  client: &Nanocld,
  args: &CargoArgs,
  pargs: &CargoPatchArgs,
  item: &CargoPatchPartial,
) -> Result<(), CliError> {
  let cargo = client
    .update_cargo(&pargs.name, args.namespace.to_owned(), item)
    .await?;
  println!("{:#?}", cargo);
  Ok(())
}

async fn exec_cargo_patch(
  client: &Nanocld,
  args: &CargoArgs,
  pargs: &CargoPatchArgs,
) -> Result<(), CliError> {
  match &pargs.commands {
    CargoPatchCommands::Set(item) => {
      exec_cargo_patch_set(client, args, pargs, item).await
    }
  }
}

pub async fn exec_cargo(
  client: &Nanocld,
  args: &CargoArgs,
) -> Result<(), CliError> {
  match &args.commands {
    CargoCommands::List => exec_cargo_list(client, args).await,
    CargoCommands::Create(item) => exec_cargo_create(client, args, item).await,
    CargoCommands::Remove(options) => {
      exec_cargo_remove(client, args, options).await
    }
    CargoCommands::Inspect(options) => {
      exec_cargo_inspect(client, args, options).await
    }
    CargoCommands::Patch(pargs) => exec_cargo_patch(client, args, pargs).await,
  }
}
