use std::collections::HashMap;

use futures::{SinkExt, StreamExt, channel::mpsc, stream::FuturesUnordered};
use ntex::rt;

use nanocl_error::io::{IoError, IoResult};
use nanocld_client::{
  NanocldClient,
  stubs::{
    cargo::{CargoDeleteQuery, CargoInspect, CargoSummary},
    generic::{GenericFilter, GenericListQueryNsp},
    process::{ProcessLogQuery, ProcessStatsQuery},
    system::{EventActorKind, NativeEventAction},
  },
};

use crate::{
  config::CliConfig,
  models::{
    CargoArg, CargoCommand, CargoCreateOpts, CargoHistoryOpts, CargoLogsOpts,
    CargoPatchOpts, CargoRestartOpts, CargoRevertOpts, CargoRow, CargoRunOpts,
    CargoStatsOpts, GenericRemoveForceOpts, GenericRemoveOpts, ProcessStatsRow,
    build_cargo_patch,
  },
  utils,
};

use super::{
  GenericCommand, GenericCommandInspect, GenericCommandLs, GenericCommandRm,
  GenericCommandStart, GenericCommandStop,
};

impl GenericCommand for CargoArg {
  fn object_name() -> &'static str {
    "cargoes"
  }
}

impl GenericCommandLs for CargoArg {
  type Item = CargoRow;
  type Args = CargoArg;
  type ApiItem = CargoSummary;

  fn get_key(item: &Self::Item) -> String {
    item.key.clone()
  }

  fn transform_filter(
    _args: &Self::Args,
    filter: &GenericFilter,
    namespace: Option<&str>,
  ) -> impl serde::Serialize {
    GenericListQueryNsp::try_from(filter.clone())
      .unwrap()
      .with_namespace(namespace)
  }
}

impl GenericCommandRm<GenericRemoveForceOpts, CargoDeleteQuery> for CargoArg {
  fn get_query(
    opts: &GenericRemoveOpts<GenericRemoveForceOpts>,
  ) -> Option<CargoDeleteQuery>
  where
    CargoDeleteQuery: serde::Serialize,
  {
    Some(CargoDeleteQuery {
      force: Some(opts.others.force),
    })
  }
}

impl GenericCommandStart for CargoArg {}

impl GenericCommandStop for CargoArg {}

impl GenericCommandInspect for CargoArg {
  type ApiItem = CargoInspect;
}

async fn wait_cargo_state(
  key: &str,
  action: NativeEventAction,
  client: &NanocldClient,
) -> IoResult<rt::JoinHandle<IoResult<()>>> {
  let waiter = utils::process::wait_process_state(
    key,
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
  _args: &CargoArg,
  opts: &CargoCreateOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let cargo = opts.clone().into();
  let item = client
    .create_cargo(&cargo, opts.namespace.as_deref())
    .await?;
  println!("{}", &item.spec.cargo_key);
  Ok(())
}

/// Execute the `nanocl cargo restart` command to restart a cargo
async fn exec_cargo_restart(
  cli_conf: &CliConfig,
  _args: &CargoArg,
  opts: &CargoRestartOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  for key in &opts.keys {
    let waiter =
      wait_cargo_state(key, NativeEventAction::Restart, client).await?;
    client.restart_process("cargo", key).await?;
    waiter.await.map_err(|err| {
      IoError::interrupted("wait_cargo_state", &err.to_string())
    })??;
  }
  Ok(())
}

/// Execute the `nanocl cargo patch` command to patch a cargo
async fn exec_cargo_patch(
  cli_conf: &CliConfig,
  _args: &CargoArg,
  opts: &CargoPatchOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let cargo = client.inspect_cargo(&opts.key).await?;
  let patch = build_cargo_patch(opts, &cargo.spec.containers)?;
  let waiter =
    wait_cargo_state(&opts.key, NativeEventAction::Update, client).await?;
  client.patch_cargo(&opts.key, &patch).await?;
  waiter.await.map_err(|err| {
    IoError::interrupted("wait_cargo_state", &err.to_string())
  })??;
  Ok(())
}

/// Execute the `nanocl cargo history` command to list the history of a cargo
async fn exec_cargo_history(
  cli_conf: &CliConfig,
  _args: &CargoArg,
  opts: &CargoHistoryOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let histories = client.list_history_cargo(&opts.key).await?;
  utils::print::print_yml(histories)?;
  Ok(())
}

/// Execute the `nanocl cargo logs` command to list the logs of a cargo
async fn exec_cargo_logs(
  cli_conf: &CliConfig,
  _args: &CargoArg,
  opts: &CargoLogsOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let query = ProcessLogQuery {
    tail: opts.tail.clone(),
    since: opts.since,
    until: opts.until,
    follow: Some(opts.follow),
    timestamps: Some(opts.timestamps),
    stderr: Some(true),
    stdout: Some(true),
  };
  let stream = client
    .logs_processes("cargo", &opts.key, Some(&query))
    .await?;
  utils::print::logs_process_stream(stream).await?;
  Ok(())
}

/// Execute the `nanocl cargo stats` command to list the stats of a cargo
async fn exec_cargo_stats(
  cli_conf: &CliConfig,
  _args: &CargoArg,
  opts: &CargoStatsOpts,
) -> IoResult<()> {
  let client = cli_conf.client.clone();
  let query = ProcessStatsQuery {
    stream: if opts.no_stream { Some(false) } else { None },
    one_shot: Some(false),
  };
  let mut stats_cargoes = HashMap::new();
  let (tx, mut rx) = mpsc::unbounded();
  let futures = opts
    .keys
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
  _args: &CargoArg,
  opts: &CargoRevertOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let waiter =
    wait_cargo_state(&opts.key, NativeEventAction::Update, client).await?;
  let cargo = client.revert_cargo(&opts.key, &opts.history_id).await?;
  waiter.await.map_err(|err| {
    IoError::interrupted("wait_cargo_state", &err.to_string())
  })??;
  utils::print::print_yml(cargo)?;
  Ok(())
}

/// Execute the `nanocl cargo run` command to run a cargo
async fn exec_cargo_run(
  cli_conf: &CliConfig,
  _args: &CargoArg,
  opts: &CargoRunOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let cargo = client
    .create_cargo(&opts.clone().into(), opts.namespace.as_deref())
    .await?;
  let waiter =
    wait_cargo_state(&cargo.spec.cargo_key, NativeEventAction::Start, client)
      .await?;
  client.start_process("cargo", &cargo.spec.cargo_key).await?;
  waiter.await.map_err(|err| {
    IoError::interrupted("wait_cargo_state", &err.to_string())
  })??;
  Ok(())
}

/// Function that execute when running `nanocl cargo`
pub async fn exec_cargo(cli_conf: &CliConfig, args: &CargoArg) -> IoResult<()> {
  match &args.command {
    CargoCommand::List(opts) => {
      CargoArg::exec_ls(
        &cli_conf.client,
        args,
        &opts.list,
        opts.namespace.as_deref(),
      )
      .await
    }
    CargoCommand::Create(opts) => exec_cargo_create(cli_conf, args, opts).await,
    CargoCommand::Remove(opts) => {
      CargoArg::exec_rm(&cli_conf.client, opts).await
    }
    CargoCommand::Start(opts) => {
      CargoArg::exec_start(&cli_conf.client, opts).await
    }
    CargoCommand::Stop(opts) => {
      CargoArg::exec_stop(&cli_conf.client, opts).await
    }
    CargoCommand::Patch(opts) => exec_cargo_patch(cli_conf, args, opts).await,
    CargoCommand::Inspect(opts) => CargoArg::exec_inspect(cli_conf, opts).await,
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn quiet_output_uses_canonical_key() {
    let row = CargoRow {
      key: "system.same".to_owned(),
      containers: String::new(),
      status: String::new(),
      instances: String::new(),
      version: String::new(),
      created_at: String::new(),
      updated_at: String::new(),
    };
    assert_eq!(<CargoArg as GenericCommandLs>::get_key(&row), "system.same");
  }
}
