use nanocl_error::io::IoResult;
use nanocld_client::stubs::generic::{GenericFilter, GenericClause};

use crate::config::CliConfig;
use crate::models::{ProcessOpts, ProcessRow};
use crate::utils::print::print_table;

/// Function that execute when running `nanocl ps`
/// Will print the list of existing instances of cargoes and virtual machines
pub async fn exec_process(
  cli_conf: &CliConfig,
  args: &ProcessOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let mut filter = GenericFilter::new();

  if !args.all {
    filter = filter.r#where(
      "data",
      GenericClause::Contains(serde_json::json!({
        "State": {
          "Status": "running"
        }
      })),
    );
  }
  if let Some(limit) = args.limit {
    filter = filter.limit(limit);
  }
  if let Some(offset) = args.offset {
    filter = filter.offset(offset);
  }
  if let Some(kind) = &args.kind {
    filter = filter.r#where("kind", GenericClause::Eq(kind.clone()));
  }
  if let Some(namespace) = &args.namespace {
    filter = filter.r#where(
      "data",
      GenericClause::Contains(serde_json::json!({
        "Config": {
          "Labels": {
            "io.nanocl.n": namespace
          }
        }
      })),
    );
  }
  let items = client.list_process(Some(&filter)).await?;
  let rows = items
    .into_iter()
    .map(ProcessRow::from)
    .collect::<Vec<ProcessRow>>();

  print_table(rows);

  Ok(())
}
