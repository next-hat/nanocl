use nanocl_error::io::IoResult;
use nanocld_client::stubs::metric::Metric;

use crate::{
  config::CliConfig,
  models::{MetricArg, MetricRow, MetricCommand},
};

use super::GenericList;

impl GenericList for MetricArg {
  type Item = MetricRow;
  type Args = MetricArg;
  type ApiItem = Metric;

  fn object_name() -> &'static str {
    "metrics"
  }

  fn get_key(item: &Self::Item) -> String {
    item.key.clone()
  }
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
  }
}
