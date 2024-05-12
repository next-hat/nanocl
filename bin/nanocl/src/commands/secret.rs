use bollard_next::secret::Secret;
use nanocl_error::io::IoResult;

use crate::{
  config::CliConfig,
  models::{
    GenericDefaultOpts, SecretArg, SecretCommand, SecretCreateOpts, SecretRow,
  },
};

use super::{
  GenericCommand, GenericCommandInspect, GenericCommandLs, GenericCommandRm,
};

impl GenericCommand for SecretArg {
  fn object_name() -> &'static str {
    "secrets"
  }
}

impl GenericCommandLs for SecretArg {
  type Item = SecretRow;
  type Args = SecretArg;
  type ApiItem = nanocld_client::stubs::secret::Secret;

  fn get_key(item: &Self::Item) -> String {
    item.name.clone()
  }
}

impl GenericCommandRm<GenericDefaultOpts, String> for SecretArg {}

impl GenericCommandInspect for SecretArg {
  type ApiItem = Secret;
}

async fn exec_secret_create(
  cli_conf: &CliConfig,
  opts: &SecretCreateOpts,
) -> IoResult<()> {
  let secret = opts.clone().try_into()?;
  cli_conf.client.create_secret(&secret).await?;
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
    SecretCommand::Remove(opts) => {
      SecretArg::exec_rm(&cli_conf.client, opts, None).await
    }
    SecretCommand::Inspect(opts) => {
      SecretArg::exec_inspect(cli_conf, opts, None).await
    }
    SecretCommand::Create(opts) => exec_secret_create(cli_conf, opts).await,
  }
}
