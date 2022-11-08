use crate::client::Nanocld;

use crate::models::{CargoInstanceArgs, CargoInstanceCommands};

use super::errors::CliError;
use super::utils::print_table;

pub async fn exec_cargo_instance(
  client: &Nanocld,
  args: &CargoInstanceArgs,
) -> Result<(), CliError> {
  match &args.commands {
    CargoInstanceCommands::List(opts) => {
      let data = client.list_containers(opts).await?;
      print_table(data);
      Ok(())
    }
  }
}
