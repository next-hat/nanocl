use crate::{
  config::CliConfig,
  models::{ProcessStatsRow, StatsOpts},
  utils,
};
use futures::{StreamExt, channel::mpsc};
use nanocl_error::io::IoResult;
use std::collections::HashMap;

pub async fn exec_stats(
  cli_config: &CliConfig,
  stats_arg: &StatsOpts,
) -> IoResult<()> {
  let client = cli_config.client.clone();
  let terminal = dialoguer::console::Term::stdout();
  let (tx_stat, mut recv_stat) = mpsc::unbounded();
  {
    let proc_list = stats_arg.names.clone();
    let client = client.clone();
    ntex::rt::spawn(async move {
      let fut = proc_list.iter().map(|x| async {
        let Ok(mut stream) = client.stats_process_by_name(x).await else {
          return;
        };
        while let Some(stat_res) = stream.next().await {
          if let Ok(stat) = stat_res {
            let _ = tx_stat.unbounded_send(stat);
          }
        }
      });
      futures::future::join_all(fut).await;
    });
  }
  let mut proc_map = HashMap::new();
  while let Some(stat) = recv_stat.next().await {
    proc_map.insert(stat.name.clone(), stat.clone());
    terminal.clear_screen()?;
    utils::print::print_table(
      proc_map.values().map(|x| ProcessStatsRow::from(x.clone())),
    );
  }
  Ok(())
}
