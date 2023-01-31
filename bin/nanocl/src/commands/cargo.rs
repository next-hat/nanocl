use bollard::exec::CreateExecOptions;
use futures::StreamExt;
use nanocld_client::NanoclClient;

use nanocl_stubs::cargo::ExecOutputKind;

use crate::error::CliError;
use crate::models::{
  CargoArgs, CargoCreateOpts, CargoCommands, CargoDeleteOpts, CargoRow,
  CargoStartOpts, CargoStopOpts, CargoPatchOpts, CargoInspectOpts,
  CargoExecOpts, CargoHistoryOpts, CargoResetOpts,
};

use super::cargo_image;
use super::utils::print_table;

/// Execute cargo command
///
/// ## Arguments
/// * [client](NanoclClient) - Nanocl client
/// * [args](CargoArgs) - Cargo arguments
/// * [options](CargoCommands) - Cargo command
///
/// ## Returns
/// * [Result](Result) - Result of the operation
///   * [Ok](Ok) - Operation was successful
///   * [Err](CliError) - Operation failed
///
async fn exec_cargo_create(
  client: &NanoclClient,
  args: &CargoArgs,
  options: &CargoCreateOpts,
) -> Result<(), CliError> {
  let cargo = options.to_owned().into();
  let item = client
    .create_cargo(&cargo, args.namespace.to_owned())
    .await?;
  println!("{}", &item.key);
  Ok(())
}

async fn exec_cargo_delete(
  client: &NanoclClient,
  args: &CargoArgs,
  options: &CargoDeleteOpts,
) -> Result<(), CliError> {
  for name in &options.names {
    client.delete_cargo(name, args.namespace.to_owned()).await?;
  }
  Ok(())
}

async fn exec_cargo_list(
  client: &NanoclClient,
  args: &CargoArgs,
) -> Result<(), CliError> {
  let items = client.list_cargo(args.namespace.to_owned()).await?;

  let rows = items
    .into_iter()
    .map(CargoRow::from)
    .collect::<Vec<CargoRow>>();
  print_table(rows);
  Ok(())
}

async fn exec_cargo_start(
  client: &NanoclClient,
  args: &CargoArgs,
  options: &CargoStartOpts,
) -> Result<(), CliError> {
  client
    .start_cargo(&options.name, args.namespace.to_owned())
    .await?;
  Ok(())
}

async fn exec_cargo_stop(
  client: &NanoclClient,
  args: &CargoArgs,
  options: &CargoStopOpts,
) -> Result<(), CliError> {
  client
    .stop_cargo(&options.name, args.namespace.to_owned())
    .await?;
  Ok(())
}

async fn exec_cargo_patch(
  client: &NanoclClient,
  args: &CargoArgs,
  options: &CargoPatchOpts,
) -> Result<(), CliError> {
  let cargo = options.to_owned().into();
  client
    .patch_cargo(&options.name, cargo, args.namespace.to_owned())
    .await?;
  Ok(())
}

async fn exec_cargo_inspect(
  client: &NanoclClient,
  args: &CargoArgs,
  options: &CargoInspectOpts,
) -> Result<(), CliError> {
  let cargo = client
    .inspect_cargo(&options.name, args.namespace.to_owned())
    .await?;
  let cargo = serde_yaml::to_string(&cargo)?;
  println!("{cargo}");
  Ok(())
}

async fn exec_cargo_exec(
  client: &NanoclClient,
  args: &CargoArgs,
  options: &CargoExecOpts,
) -> Result<(), CliError> {
  let exec: CreateExecOptions<String> = options.to_owned().into();
  let mut stream = client
    .exec_cargo(&options.name, exec, args.namespace.to_owned())
    .await?;

  while let Some(output) = stream.next().await {
    match output.kind {
      ExecOutputKind::StdOut => {
        print!("{}", &output.data);
      }
      ExecOutputKind::StdErr => {
        eprint!("{}", output.data);
      }
      ExecOutputKind::StdIn => println!("TODO: StdIn {}", &output.data),
      ExecOutputKind::Console => print!("{}", &output.data),
    }
  }

  Ok(())
}

async fn exec_cargo_history(
  client: &NanoclClient,
  args: &CargoArgs,
  opts: &CargoHistoryOpts,
) -> Result<(), CliError> {
  let histories = client
    .list_history_cargo(&opts.name, args.namespace.to_owned())
    .await?;

  let histories = serde_yaml::to_string(&histories)?;
  println!("{histories}");
  Ok(())
}

async fn exec_cargo_reset(
  client: &NanoclClient,
  args: &CargoArgs,
  opts: &CargoResetOpts,
) -> Result<(), CliError> {
  let cargo = client
    .reset_cargo(&opts.name, &opts.history_id, args.namespace.to_owned())
    .await?;
  let cargo = serde_yaml::to_string(&cargo)?;
  println!("{cargo}");
  Ok(())
}

pub async fn exec_cargo(
  client: &NanoclClient,
  args: &CargoArgs,
) -> Result<(), CliError> {
  match &args.commands {
    CargoCommands::List => exec_cargo_list(client, args).await,
    CargoCommands::Create(options) => {
      exec_cargo_create(client, args, options).await
    }
    CargoCommands::Remove(options) => {
      exec_cargo_delete(client, args, options).await
    }
    CargoCommands::Image(options) => {
      cargo_image::exec_cargo_image(client, options).await
    }
    CargoCommands::Start(options) => {
      exec_cargo_start(client, args, options).await
    }
    CargoCommands::Stop(options) => {
      exec_cargo_stop(client, args, options).await
    }
    CargoCommands::Patch(options) => {
      exec_cargo_patch(client, args, options).await
    }
    CargoCommands::Inspect(options) => {
      exec_cargo_inspect(client, args, options).await
    }
    CargoCommands::Exec(options) => {
      exec_cargo_exec(client, args, options).await
    }
    CargoCommands::History(opts) => {
      exec_cargo_history(client, args, opts).await
    }
    CargoCommands::Reset(options) => {
      exec_cargo_reset(client, args, options).await
    }
  }
}
