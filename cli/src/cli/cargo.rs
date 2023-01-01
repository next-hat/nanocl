use crate::client::Nanocld;

use nanocl_models::cargo::CargoPartial;
use nanocl_models::cargo_config::CargoConfigPartial;

use crate::models::{CargoArgs, CargoCreateOpts, CargoCommands, CargoDeleteOpts};

use super::errors::CliError;

async fn exec_cargo_create(
  client: &Nanocld,
  args: &CargoArgs,
  options: &CargoCreateOpts,
) -> Result<(), CliError> {
  let cargo = CargoPartial {
    name: options.name.to_owned(),
    config: CargoConfigPartial {
      name: options.name.to_owned(),
      container: bollard::container::Config {
        image: Some(options.image.to_owned()),
        ..Default::default()
      },
      ..Default::default()
    },
  };
  let item = client
    .create_cargo(&cargo, args.namespace.to_owned())
    .await?;
  println!("{}", item.key);
  Ok(())
}

async fn exec_cargo_delete(
  client: &Nanocld,
  args: &CargoArgs,
  options: &CargoDeleteOpts,
) -> Result<(), CliError> {
  for name in &options.names {
    client
      .delete_cargo(name.to_owned(), args.namespace.to_owned())
      .await?;
  }
  Ok(())
}

pub async fn exec_cargo(
  client: &Nanocld,
  args: &CargoArgs,
) -> Result<(), CliError> {
  match &args.commands {
    CargoCommands::Create(options) => {
      exec_cargo_create(client, args, options).await
    }
    CargoCommands::Remove(options) => {
      exec_cargo_delete(client, args, options).await
    }
    _ => todo!("Not implemented yet"),
  }
}
