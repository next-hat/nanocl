use std::{process, collections::HashMap};

use ntex::rt;
use futures::{SinkExt, StreamExt, channel::mpsc, stream::FuturesUnordered};
use bollard_next::exec::{CreateExecOptions, StartExecOptions};

use nanocl_error::io::IoResult;
use nanocld_client::{
  NanocldClient,
  stubs::{
    cargo::{CargoDeleteQuery, CargoSummary},
    generic::{GenericFilter, GenericListNspQuery},
    process::{OutputKind, ProcessLogQuery, ProcessStatsQuery},
    system::{EventActorKind, NativeEventAction},
  },
};

use crate::{
  utils,
  config::CliConfig,
  models::{
    GenericRemoveForceOpts, GenericRemoveOpts, CargoArg, CargoCreateOpts,
    CargoCommand, CargoRow, CargoStartOpts, CargoStopOpts, CargoPatchOpts,
    CargoInspectOpts, CargoExecOpts, CargoHistoryOpts, CargoRevertOpts,
    CargoLogsOpts, CargoRunOpts, CargoRestartOpts, CargoStatsOpts,
    ProcessStatsRow,
  },
};

use super::{GenericList, GenericRemove};

impl GenericList for CargoArg {
  type Item = CargoRow;
  type Args = CargoArg;
  type ApiItem = CargoSummary;

  fn object_name() -> &'static str {
    "cargoes"
  }

  fn get_key(item: &Self::Item) -> String {
    item.name.clone()
  }

  fn transform_filter(
    args: &Self::Args,
    filter: &GenericFilter,
  ) -> impl serde::Serialize {
    GenericListNspQuery::try_from(filter.clone())
      .unwrap()
      .with_namespace(args.namespace.as_deref())
  }
}

impl GenericRemove<GenericRemoveForceOpts, CargoDeleteQuery> for CargoArg {
  fn object_name() -> &'static str {
    "cargoes"
  }

  fn get_query(
    opts: &GenericRemoveOpts<GenericRemoveForceOpts>,
    namespace: Option<String>,
  ) -> Option<CargoDeleteQuery>
  where
    CargoDeleteQuery: serde::Serialize,
  {
    Some(CargoDeleteQuery {
      namespace,
      force: Some(opts.others.force),
    })
  }
}

async fn wait_cargo_state(
  name: &str,
  args: &CargoArg,
  action: NativeEventAction,
  client: &NanocldClient,
) -> IoResult<rt::JoinHandle<IoResult<()>>> {
  let waiter = utils::process::wait_process_state(
    &format!("{}.{}", name, args.namespace.as_deref().unwrap_or("global")),
    EventActorKind::Cargo,
    [action].to_vec(),
    client,
  )
  .await?;
  Ok(waiter)
}

/// Execute the `nanocl cargo create` command to create a new cargo
async fn exec_cargo_create(
  cli_conf: &CliConfig,
  args: &CargoArg,
  opts: &CargoCreateOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let cargo = opts.clone().into();
  let item = client
    .create_cargo(&cargo, args.namespace.as_deref())
    .await?;
  println!("{}", &item.spec.cargo_key);
  Ok(())
}

/// Execute the `nanocl cargo start` command to start a cargo
async fn exec_cargo_start(
  cli_conf: &CliConfig,
  args: &CargoArg,
  opts: &CargoStartOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let waiter =
    wait_cargo_state(&opts.name, args, NativeEventAction::Start, client)
      .await?;
  client
    .start_process("cargo", &opts.name, args.namespace.as_deref())
    .await?;
  waiter.await??;
  Ok(())
}

/// Execute the `nanocl cargo stop` command to stop a cargo
async fn exec_cargo_stop(
  cli_conf: &CliConfig,
  args: &CargoArg,
  opts: &CargoStopOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  for name in &opts.names {
    let waiter =
      wait_cargo_state(name, args, NativeEventAction::Stop, client).await?;
    if let Err(err) = client
      .stop_process("cargo", name, args.namespace.as_deref())
      .await
    {
      eprintln!("{name}: {err}");
    }
    let _ = waiter.await?;
  }
  Ok(())
}

/// Execute the `nanocl cargo restart` command to restart a cargo
async fn exec_cargo_restart(
  cli_conf: &CliConfig,
  args: &CargoArg,
  opts: &CargoRestartOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  for name in &opts.names {
    client
      .restart_process("cargo", name, args.namespace.as_deref())
      .await?;
  }
  Ok(())
}

/// Execute the `nanocl cargo patch` command to patch a cargo
async fn exec_cargo_patch(
  cli_conf: &CliConfig,
  args: &CargoArg,
  opts: &CargoPatchOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let waiter =
    wait_cargo_state(&opts.name, args, NativeEventAction::Start, client)
      .await?;
  client
    .patch_cargo(&opts.name, &opts.clone().into(), args.namespace.as_deref())
    .await?;
  waiter.await??;
  Ok(())
}

/// Execute the `nanocl cargo inspect` command to inspect a cargo
async fn exec_cargo_inspect(
  cli_conf: &CliConfig,
  args: &CargoArg,
  opts: &CargoInspectOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let cargo = client
    .inspect_cargo(&opts.name, args.namespace.as_deref())
    .await?;
  let display = opts
    .display
    .clone()
    .unwrap_or(cli_conf.user_config.display_format.clone());
  utils::print::display_format(&display, cargo)?;
  Ok(())
}

/// Execute the `nanocl cargo exec` command to execute a command in a cargo
async fn exec_cargo_exec(
  cli_conf: &CliConfig,
  args: &CargoArg,
  opts: &CargoExecOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let exec: CreateExecOptions = opts.clone().into();
  let result = client
    .create_exec(&opts.name, &exec, args.namespace.as_deref())
    .await?;
  let mut stream = client
    .start_exec(
      &result.id,
      &StartExecOptions {
        tty: opts.tty,
        ..Default::default()
      },
    )
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
  let exec_infos = client.inspect_exec(&result.id).await?;
  match exec_infos.exit_code {
    Some(code) => {
      if code == 0 {
        return Ok(());
      }
      process::exit(code.try_into().unwrap_or(1))
    }
    None => Ok(()),
  }
}

/// Execute the `nanocl cargo history` command to list the history of a cargo
async fn exec_cargo_history(
  cli_conf: &CliConfig,
  args: &CargoArg,
  opts: &CargoHistoryOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let histories = client
    .list_history_cargo(&opts.name, args.namespace.as_deref())
    .await?;
  utils::print::print_yml(histories)?;
  Ok(())
}

/// Execute the `nanocl cargo logs` command to list the logs of a cargo
async fn exec_cargo_logs(
  cli_conf: &CliConfig,
  args: &CargoArg,
  opts: &CargoLogsOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let query = ProcessLogQuery {
    namespace: args.namespace.clone(),
    tail: opts.tail.clone(),
    since: opts.since,
    until: opts.until,
    follow: Some(opts.follow),
    timestamps: Some(opts.timestamps),
    stderr: None,
    stdout: None,
  };
  let stream = client
    .logs_processes("cargo", &opts.name, Some(&query))
    .await?;
  utils::print::logs_process_stream(stream).await?;
  Ok(())
}

/// Execute the `nanocl cargo stats` command to list the stats of a cargo
async fn exec_cargo_stats(
  cli_conf: &CliConfig,
  args: &CargoArg,
  opts: &CargoStatsOpts,
) -> IoResult<()> {
  let client = cli_conf.client.clone();
  let query = ProcessStatsQuery {
    namespace: args.namespace.clone(),
    stream: if opts.no_stream { Some(false) } else { None },
    one_shot: Some(false),
  };
  let mut stats_cargoes = HashMap::new();
  let (tx, mut rx) = mpsc::unbounded();
  let futures = opts
    .names
    .iter()
    .map(|name| {
      let name = name.clone();
      let query = query.clone();
      let mut tx = tx.clone();
      let client = client.clone();
      async move {
        let Ok(mut stream) =
          client.stats_processes("cargo", &name, Some(&query)).await
        else {
          return;
        };
        while let Some(stats) = stream.next().await {
          let stats = match stats {
            Ok(stats) => stats,
            Err(e) => {
              eprintln!("Error: {e}");
              break;
            }
          };
          if let Err(err) = tx.send(stats).await {
            eprintln!("Error: {err}");
            break;
          }
        }
      }
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>();
  rt::spawn(futures);
  while let Some(stats) = rx.next().await {
    stats_cargoes.insert(stats.name.clone(), stats.clone());
    // convert stats_cargoes in a Arrays of CargoStatsRow
    let stats = stats_cargoes
      .values()
      .map(|stats| ProcessStatsRow::from(stats.clone()))
      .collect::<Vec<ProcessStatsRow>>();
    // clear terminal
    let term = dialoguer::console::Term::stdout();
    let _ = term.clear_screen();
    utils::print::print_table(stats);
  }
  Ok(())
}

/// Execute the `nanocl cargo revert` command to revert a cargo to a previous state
async fn exec_cargo_revert(
  cli_conf: &CliConfig,
  args: &CargoArg,
  opts: &CargoRevertOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let cargo = client
    .revert_cargo(&opts.name, &opts.history_id, args.namespace.as_deref())
    .await?;
  utils::print::print_yml(cargo)?;
  Ok(())
}

/// Execute the `nanocl cargo run` command to run a cargo
async fn exec_cargo_run(
  cli_conf: &CliConfig,
  args: &CargoArg,
  opts: &CargoRunOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let waiter =
    wait_cargo_state(&opts.name, args, NativeEventAction::Start, client)
      .await?;
  let cargo = client
    .create_cargo(&opts.clone().into(), args.namespace.as_deref())
    .await?;
  client
    .start_process("cargo", &cargo.spec.name, Some(&cargo.namespace_name))
    .await?;
  waiter.await??;
  Ok(())
}

/// Function that execute when running `nanocl cargo`
pub async fn exec_cargo(cli_conf: &CliConfig, args: &CargoArg) -> IoResult<()> {
  match &args.command {
    CargoCommand::List(opts) => {
      CargoArg::exec_ls(&cli_conf.client, args, opts).await
    }
    CargoCommand::Create(opts) => exec_cargo_create(cli_conf, args, opts).await,
    CargoCommand::Remove(opts) => {
      CargoArg::exec_rm(
        &cli_conf.client,
        opts,
        Some(args.namespace.clone().unwrap_or("global".to_owned())),
      )
      .await
    }
    CargoCommand::Start(opts) => exec_cargo_start(cli_conf, args, opts).await,
    CargoCommand::Stop(opts) => exec_cargo_stop(cli_conf, args, opts).await,
    CargoCommand::Patch(opts) => exec_cargo_patch(cli_conf, args, opts).await,
    CargoCommand::Inspect(opts) => {
      exec_cargo_inspect(cli_conf, args, opts).await
    }
    CargoCommand::Exec(opts) => exec_cargo_exec(cli_conf, args, opts).await,
    CargoCommand::History(opts) => {
      exec_cargo_history(cli_conf, args, opts).await
    }
    CargoCommand::Revert(opts) => exec_cargo_revert(cli_conf, args, opts).await,
    CargoCommand::Logs(opts) => exec_cargo_logs(cli_conf, args, opts).await,
    CargoCommand::Run(opts) => exec_cargo_run(cli_conf, args, opts).await,
    CargoCommand::Restart(opts) => {
      exec_cargo_restart(cli_conf, args, opts).await
    }
    CargoCommand::Stats(opts) => exec_cargo_stats(cli_conf, args, opts).await,
  }
}
