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
/// * [opts](CargoCommands) - Cargo command
///
/// ## Returns
/// * [Result](Result) - Result of the operation
///   * [Ok](Ok) - Operation was successful
///   * [Err](IoError) - Operation failed
///
async fn exec_cargo_create(
  client: &NanocldClient,
  args: &CargoArgs,
  opts: &CargoCreateOpts,
) -> IoResult<()> {
  let cargo = opts.clone().into();
  let item = client.create_cargo(&cargo, args.namespace.clone()).await?;
  println!("{}", &item.key);
  Ok(())
}

async fn exec_cargo_rm(
  client: &NanocldClient,
  args: &CargoArgs,
  opts: &CargoRemoveOpts,
) -> IoResult<()> {
  if !opts.skip_confirm {
    utils::dialog::confirm(&format!("Delete cargo  {}?", opts.names.join(",")))
      .map_err(|err| err.map_err_context(|| "Delete cargo images"))?;
  }
  let query = CargoDeleteQuery {
    namespace: args.namespace.clone(),
    force: Some(opts.force),
  };
  for name in &opts.names {
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
  opts: &CargoStartOpts,
) -> IoResult<()> {
  client
    .start_cargo(&opts.name, args.namespace.clone())
    .await?;
  Ok(())
}

async fn exec_cargo_stop(
  client: &NanocldClient,
  args: &CargoArgs,
  opts: &CargoStopOpts,
) -> IoResult<()> {
  for name in &opts.names {
    client.stop_cargo(name, args.namespace.clone()).await?;
  }
  Ok(())
}

async fn exec_cargo_restart(
  client: &NanocldClient,
  args: &CargoArgs,
  opts: &CargoRestartOpts,
) -> IoResult<()> {
  for name in &opts.names {
    client.restart_cargo(name, args.namespace.clone()).await?;
  }
  Ok(())
}

async fn exec_cargo_patch(
  client: &NanocldClient,
  args: &CargoArgs,
  opts: &CargoPatchOpts,
) -> IoResult<()> {
  let cargo = opts.clone().into();
  client
    .patch_cargo(&opts.name, cargo, args.namespace.clone())
    .await?;
  Ok(())
}

async fn exec_cargo_inspect(
  client: &NanocldClient,
  args: &CargoArgs,
  opts: &CargoInspectOpts,
) -> IoResult<()> {
  let cargo = client
    .inspect_cargo(&opts.name, args.namespace.clone())
    .await?;

  let display = opts.display.clone().unwrap_or_default();

  utils::print::display_format(&display, cargo)?;
  Ok(())
}

async fn exec_cargo_exec(
  client: &NanocldClient,
  args: &CargoArgs,
  opts: &CargoExecOpts,
) -> IoResult<()> {
  let exec: CreateExecOptions = opts.clone().into();
  let mut stream = client
    .exec_cargo(&opts.name, exec, args.namespace.clone())
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
  opts: &CargoLogsOpts,
) -> IoResult<()> {
  let query = CargoLogQuery {
    namespace: args.namespace.clone(),
    tail: opts.tail.clone(),
    since: opts.since,
    until: opts.until,
    follow: Some(opts.follow),
    timestamps: Some(opts.timestamps),
    stderr: None,
    stdout: None,
  };
  let mut stream = client.logs_cargo(&opts.name, &query).await?;
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
    CargoCommands::Create(opts) => exec_cargo_create(client, args, opts).await,
    CargoCommands::Remove(opts) => exec_cargo_rm(client, args, opts).await,
    CargoCommands::Image(opts) => {
      cargo_image::exec_cargo_image(client, opts).await
    }
    CargoCommands::Start(opts) => exec_cargo_start(client, args, opts).await,
    CargoCommands::Stop(opts) => exec_cargo_stop(client, args, opts).await,
    CargoCommands::Patch(opts) => exec_cargo_patch(client, args, opts).await,
    CargoCommands::Inspect(opts) => {
      exec_cargo_inspect(client, args, opts).await
    }
    CargoCommands::Exec(opts) => exec_cargo_exec(client, args, opts).await,
    CargoCommands::History(opts) => {
      exec_cargo_history(client, args, opts).await
    }
    CargoCommands::Revert(opts) => exec_cargo_revert(client, args, opts).await,
    CargoCommands::Logs(opts) => exec_cargo_logs(client, args, opts).await,
    CargoCommands::Run(opts) => exec_cargo_run(client, args, opts).await,
    CargoCommands::Restart(opts) => {
      exec_cargo_restart(client, args, opts).await
    }
  }
}
