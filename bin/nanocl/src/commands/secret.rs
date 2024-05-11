use nanocl_error::io::IoResult;

use crate::{
  utils,
  config::CliConfig,
  models::{
    GenericDefaultOpts, SecretArg, SecretCommand, SecretCreateOpts,
    SecretInspectOpts, SecretRow,
  },
};

use super::{GenericList, GenericDelete};

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

impl GenericDelete<GenericDefaultOpts, String> for SecretArg {
  fn object_name() -> &'static str {
    "secrets"
  }
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
      SecretArg::exec_rm(&cli_conf.client, opts).await
    }
    SecretCommand::Inspect(opts) => exec_secret_inspect(cli_conf, opts).await,
    SecretCommand::Create(opts) => exec_secret_create(cli_conf, opts).await,
  }
}
