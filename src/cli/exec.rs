use crate::client::Nanocld;
use crate::models::{ExecArgs, CargoInstanceExecQuery};

use super::errors::CliError;

pub async fn exec_exec(
  client: &Nanocld,
  args: &ExecArgs,
) -> Result<(), CliError> {
  let config = CargoInstanceExecQuery {
    attach_stdin: None,
    attach_stdout: Some(true),
    attach_stderr: Some(true),
    detach_keys: None,
    tty: Some(true),
    env: None,
    cmd: Some(args.cmd.to_owned()),
    privileged: None,
    user: None,
    working_dir: None,
  };

  let exec = client.create_exec(&args.name, config).await?;

  client.start_exec(&exec.id).await?;
  Ok(())
}
