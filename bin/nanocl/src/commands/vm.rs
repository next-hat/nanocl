use nanocld_client::NanocldClient;

use crate::error::CliError;
use crate::models::{VmArgs, VmCommands};

use super::vm_image::exec_vm_image;

pub async fn exec_vm(
  client: &NanocldClient,
  args: &VmArgs,
) -> Result<(), CliError> {
  match &args.commands {
    VmCommands::Image(args) => exec_vm_image(client, args).await,
  }
}
