use nanocl_error::io::{IoResult, FromIo};

use crate::utils;
use crate::config::CliConfig;
use crate::models::{
  SecretArg, SecretCommand, SecretRow, SecretRemoveOpts, SecretInspectOpts,
};

use super::GenericList;

impl GenericList for SecretArg {
  type Item = SecretRow;
  type Args = SecretArg;
  type ApiItem = nanocld_client::stubs::secret::Secret;

  fn object_name() -> &'static str {
    "secrets"
  }

  fn get_key(item: &Self::Item) -> String {
    item.name.clone()
  }
}

/// Function that execute when running `nanocl secret rm`
async fn exec_secret_rm(
  cli_conf: &CliConfig,
  opts: &SecretRemoveOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  if !opts.skip_confirm {
    utils::dialog::confirm(&format!("Delete secret {}?", opts.keys.join(",")))
      .map_err(|err| err.map_err_context(|| "Delete secret"))?;
  }
  for key in &opts.keys {
    client.delete_secret(key).await?;
  }
  Ok(())
}

/// Function that execute when running `nanocl secret inspect`
async fn exec_secret_inspect(
  cli_conf: &CliConfig,
  opts: &SecretInspectOpts,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let secret = client.inspect_secret(&opts.key).await?;
  let _ = utils::print::display_format(
    &opts.display.clone().unwrap_or_default(),
    secret,
  );
  Ok(())
}

/// Function that execute when running `nanocl secret`
pub async fn exec_secret(
  cli_conf: &CliConfig,
  args: &SecretArg,
) -> IoResult<()> {
  match &args.command {
    SecretCommand::List(opts) => {
      SecretArg::exec_ls(&cli_conf.client, args, opts).await
    }
    SecretCommand::Remove(opts) => exec_secret_rm(cli_conf, opts).await,
    SecretCommand::Inspect(opts) => exec_secret_inspect(cli_conf, opts).await,
  }
}
