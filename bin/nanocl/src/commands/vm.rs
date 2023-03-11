use nanocld_client::NanocldClient;

use crate::error::CliError;
use crate::models::{VmArgs, VmCommands, VmCreateOpts};

use super::vm_image::exec_vm_image;

pub async fn exec_vm_create(
  client: &NanocldClient,
  args: &VmArgs,
  options: &VmCreateOpts,
) -> Result<(), CliError> {
  let vm = options.clone().into();
  let vm = client.create_vm(&vm, args.namespace.clone()).await?;

  Ok(())
}

pub async fn exec_vm(
  client: &NanocldClient,
  args: &VmArgs,
) -> Result<(), CliError> {
  match &args.commands {
    VmCommands::Image(args) => exec_vm_image(client, args).await,
    VmCommands::Create(options) => exec_vm_create(client, args, options).await,
  }
}
