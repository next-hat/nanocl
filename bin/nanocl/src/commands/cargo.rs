use futures::StreamExt;
use bollard_next::exec::CreateExecOptions;

use nanocld_client::NanoclClient;
use nanocld_client::stubs::cargo::CargoOutputKind;

use crate::utils::print::*;
use crate::error::CliError;
use crate::models::{
  CargoArgs, CargoCreateOpts, CargoCommands, CargoDeleteOpts, CargoRow,
  CargoStartOpts, CargoStopOpts, CargoPatchOpts, CargoInspectOpts,
  CargoExecOpts, CargoHistoryOpts, CargoResetOpts, CargoLogsOpts, CargoRunOpts,
};

use super::cargo_image::{self, exec_create_cargo_image};

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
  print_yml(cargo)?;
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
    let output = output?;
    match output.kind {
      CargoOutputKind::StdOut => {
        print!("{}", &output.data);
      }
      CargoOutputKind::StdErr => {
        eprint!("{}", output.data);
      }
      CargoOutputKind::StdIn => println!("TODO: StdIn {}", &output.data),
      CargoOutputKind::Console => print!("{}", &output.data),
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

async fn exec_cargo_logs(
  client: &NanoclClient,
  args: &CargoArgs,
  options: &CargoLogsOpts,
) -> Result<(), CliError> {
  let mut stream = client
    .logs_cargo(&options.name, args.namespace.to_owned())
    .await?;
  while let Some(log) = stream.next().await {
    let log = match log {
      Ok(log) => log,
      Err(e) => {
        eprintln!("Error: {e}");
        break;
      }
    };
    match log.kind {
      CargoOutputKind::StdOut => {
        print!("{}", &log.data);
      }
      CargoOutputKind::StdErr => {
        eprint!("{}", log.data);
      }
      CargoOutputKind::StdIn => println!("TODO: StdIn {}", &log.data),
      CargoOutputKind::Console => print!("{}", &log.data),
    }
  }
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

async fn exec_cargo_run(
  client: &NanoclClient,
  args: &CargoArgs,
  opts: &CargoRunOpts,
) -> Result<(), CliError> {
  // Image is not existing so we donwload it
  if client.inspect_cargo_image(&opts.image).await.is_err() {
    exec_create_cargo_image(client, &opts.image).await?;
  }

  let cargo = client
    .create_cargo(&opts.clone().into(), args.namespace.clone())
    .await?;

  client
    .start_cargo(&cargo.name, Some(cargo.namespace_name))
    .await?;

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
    CargoCommands::Logs(options) => {
      exec_cargo_logs(client, args, options).await
    }
    CargoCommands::Run(options) => exec_cargo_run(client, args, options).await,
  }
}
