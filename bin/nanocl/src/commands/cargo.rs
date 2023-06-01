use futures::StreamExt;
use bollard_next::exec::CreateExecOptions;

use nanocl_utils::io_error::{FromIo, IoResult};
use nanocld_client::NanocldClient;
use nanocld_client::stubs::cargo::{OutputKind, CargoDeleteQuery, CargoLogQuery};

use crate::utils;
use crate::models::{
  CargoArgs, CargoCreateOpts, CargoCommands, CargoRemoveOpts, CargoRow,
  CargoStartOpts, CargoStopOpts, CargoPatchOpts, CargoInspectOpts,
  CargoExecOpts, CargoHistoryOpts, CargoRevertOpts, CargoLogsOpts,
  CargoRunOpts, CargoRestartOpts, CargoListOpts,
};

use super::cargo_image::{self, exec_cargo_image_pull};

/// Execute cargo command
///
/// ## Arguments
/// * [client](NanocldClient) - Nanocl client
/// * [args](CargoArgs) - Cargo arguments
/// * [options](CargoCommands) - Cargo command
///
/// ## Returns
/// * [Result](Result) - Result of the operation
///   * [Ok](Ok) - Operation was successful
///   * [Err](IoError) - Operation failed
///
async fn exec_cargo_create(
  client: &NanocldClient,
  args: &CargoArgs,
  options: &CargoCreateOpts,
) -> IoResult<()> {
  let cargo = options.clone().into();
  let item = client.create_cargo(&cargo, args.namespace.clone()).await?;
  println!("{}", &item.key);
  Ok(())
}

async fn exec_cargo_rm(
  client: &NanocldClient,
  args: &CargoArgs,
  options: &CargoRemoveOpts,
) -> IoResult<()> {
  if !options.skip_confirm {
    utils::dialog::confirm(&format!(
      "Delete cargo  {}?",
      options.names.join(",")
    ))
    .map_err(|err| err.map_err_context(|| "Delete cargo images"))?;
  }
  let query = CargoDeleteQuery {
    namespace: args.namespace.clone(),
    force: Some(options.force),
  };
  for name in &options.names {
    client.delete_cargo(name, &query).await?;
  }
  Ok(())
}

async fn exec_cargo_ls(
  client: &NanocldClient,
  args: &CargoArgs,
  opts: &CargoListOpts,
) -> IoResult<()> {
  let items = client.list_cargo(args.namespace.clone()).await?;

  let rows = items
    .into_iter()
    .map(CargoRow::from)
    .collect::<Vec<CargoRow>>();
  match opts.quiet {
    true => {
      for row in rows {
        println!("{}", row.name);
      }
    }
    false => {
      utils::print::print_table(rows);
    }
  }
  Ok(())
}

async fn exec_cargo_start(
  client: &NanocldClient,
  args: &CargoArgs,
  options: &CargoStartOpts,
) -> IoResult<()> {
  client
    .start_cargo(&options.name, args.namespace.clone())
    .await?;
  Ok(())
}

async fn exec_cargo_stop(
  client: &NanocldClient,
  args: &CargoArgs,
  options: &CargoStopOpts,
) -> IoResult<()> {
  for name in &options.names {
    client.stop_cargo(name, args.namespace.clone()).await?;
  }
  Ok(())
}

async fn exec_cargo_restart(
  client: &NanocldClient,
  args: &CargoArgs,
  options: &CargoRestartOpts,
) -> IoResult<()> {
  for name in &options.names {
    client.restart_cargo(name, args.namespace.clone()).await?;
  }
  Ok(())
}

async fn exec_cargo_patch(
  client: &NanocldClient,
  args: &CargoArgs,
  options: &CargoPatchOpts,
) -> IoResult<()> {
  let cargo = options.clone().into();
  client
    .patch_cargo(&options.name, cargo, args.namespace.clone())
    .await?;
  Ok(())
}

async fn exec_cargo_inspect(
  client: &NanocldClient,
  args: &CargoArgs,
  options: &CargoInspectOpts,
) -> IoResult<()> {
  let cargo = client
    .inspect_cargo(&options.name, args.namespace.clone())
    .await?;
  utils::print::print_yml(cargo)?;
  Ok(())
}

async fn exec_cargo_exec(
  client: &NanocldClient,
  args: &CargoArgs,
  options: &CargoExecOpts,
) -> IoResult<()> {
  let exec: CreateExecOptions = options.clone().into();
  let mut stream = client
    .exec_cargo(&options.name, exec, args.namespace.clone())
    .await?;

  while let Some(output) = stream.next().await {
    let output = output?;
    match output.kind {
      OutputKind::StdOut => {
        print!("{}", &output.data);
      }
      OutputKind::StdErr => {
        eprint!("{}", output.data);
      }
      OutputKind::StdIn => println!("TODO: StdIn {}", &output.data),
      OutputKind::Console => print!("{}", &output.data),
    }
  }

  Ok(())
}

async fn exec_cargo_history(
  client: &NanocldClient,
  args: &CargoArgs,
  opts: &CargoHistoryOpts,
) -> IoResult<()> {
  let histories = client
    .list_history_cargo(&opts.name, args.namespace.clone())
    .await?;

  utils::print::print_yml(histories)?;
  Ok(())
}

async fn exec_cargo_logs(
  client: &NanocldClient,
  args: &CargoArgs,
  options: &CargoLogsOpts,
) -> IoResult<()> {
  let query = CargoLogQuery {
    namespace: args.namespace.clone(),
    tail: options.tail.clone(),
    since: options.since,
    until: options.until,
    follow: Some(options.follow),
    timestamps: Some(options.timestamps),
    stderr: None,
    stdout: None,
  };
  let mut stream = client.logs_cargo(&options.name, &query).await?;
  while let Some(log) = stream.next().await {
    let log = match log {
      Ok(log) => log,
      Err(e) => {
        eprintln!("Error: {e}");
        break;
      }
    };
    match log.kind {
      OutputKind::StdOut => {
        print!("{}", &log.data);
      }
      OutputKind::StdErr => {
        eprint!("{}", log.data);
      }
      OutputKind::StdIn => println!("TODO: StdIn {}", &log.data),
      OutputKind::Console => print!("{}", &log.data),
    }
  }
  Ok(())
}

async fn exec_cargo_revert(
  client: &NanocldClient,
  args: &CargoArgs,
  opts: &CargoRevertOpts,
) -> IoResult<()> {
  let cargo = client
    .revert_cargo(&opts.name, &opts.history_id, args.namespace.clone())
    .await?;
  utils::print::print_yml(cargo)?;
  Ok(())
}

async fn exec_cargo_run(
  client: &NanocldClient,
  args: &CargoArgs,
  opts: &CargoRunOpts,
) -> IoResult<()> {
  // Image is not existing so we donwload it
  if client.inspect_cargo_image(&opts.image).await.is_err() {
    exec_cargo_image_pull(client, &opts.image).await?;
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
  client: &NanocldClient,
  args: &CargoArgs,
) -> IoResult<()> {
  match &args.commands {
    CargoCommands::List(opts) => exec_cargo_ls(client, args, opts).await,
    CargoCommands::Create(options) => {
      exec_cargo_create(client, args, options).await
    }
    CargoCommands::Remove(options) => {
      exec_cargo_rm(client, args, options).await
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
    CargoCommands::Revert(options) => {
      exec_cargo_revert(client, args, options).await
    }
    CargoCommands::Logs(options) => {
      exec_cargo_logs(client, args, options).await
    }
    CargoCommands::Run(options) => exec_cargo_run(client, args, options).await,
    CargoCommands::Restart(options) => {
      exec_cargo_restart(client, args, options).await
    }
  }
}
