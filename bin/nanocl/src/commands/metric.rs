use nanocl_error::io::IoResult;
use nanocld_client::stubs::metric::Metric;

use crate::{
  config::CliConfig,
  models::{MetricArg, MetricCommand, MetricRow},
};

use super::{GenericCommand, GenericCommandInspect, GenericCommandLs};

impl GenericCommand for MetricArg {
  fn object_name() -> &'static str {
    "metrics"
  }
}

impl GenericCommandLs for MetricArg {
  type Item = MetricRow;
  type Args = MetricArg;
  type ApiItem = Metric;

  fn get_key(item: &Self::Item) -> String {
    item.key.clone()
  }
}

impl GenericCommandInspect for MetricArg {
  type ApiItem = Metric;
}

/// Function that execute when running `nanocl metric`
pub async fn exec_metric(
  cli_conf: &CliConfig,
  args: &MetricArg,
) -> IoResult<()> {
  match &args.command {
    MetricCommand::List(opts) => {
      MetricArg::exec_ls(&cli_conf.client, args, opts).await
    }
    MetricCommand::Inspect(opts) => {
      MetricArg::exec_inspect(cli_conf, opts, None).await
    }
  }
}
