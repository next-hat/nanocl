use nanocld_client::NanocldClient;

use crate::error::CliError;
use crate::models::{
  VmArgs, VmCommands, VmCreateOpts, VmRow, VmRunOpts, VmPatchOpts,
};
use crate::utils::print::{print_table, print_yml};

use super::vm_image::exec_vm_image;

pub async fn exec_vm_create(
  client: &NanocldClient,
  args: &VmArgs,
  options: &VmCreateOpts,
) -> Result<(), CliError> {
  let vm = options.clone().into();
  let vm = client.create_vm(&vm, args.namespace.clone()).await?;

  println!("{}", &vm.key);

  Ok(())
}

pub async fn exec_vm_ls(
  client: &NanocldClient,
  args: &VmArgs,
) -> Result<(), CliError> {
  let items = client.list_vm(args.namespace.clone()).await?;

  let rows = items.into_iter().map(VmRow::from).collect::<Vec<VmRow>>();

  print_table(rows);

  Ok(())
}

pub async fn exec_vm_rm(
  client: &NanocldClient,
  args: &VmArgs,
  names: &[String],
) -> Result<(), CliError> {
  for name in names {
    client.delete_vm(name, args.namespace.clone()).await?;
  }

  Ok(())
}

pub async fn exec_vm_inspect(
  client: &NanocldClient,
  args: &VmArgs,
  name: &str,
) -> Result<(), CliError> {
  let vm = client.inspect_vm(name, args.namespace.clone()).await?;

  let _ = print_yml(vm);

  Ok(())
}

pub async fn exec_vm_start(
  client: &NanocldClient,
  args: &VmArgs,
  name: &str,
) -> Result<(), CliError> {
  client.start_vm(name, args.namespace.clone()).await?;

  Ok(())
}

pub async fn exec_vm_stop(
  client: &NanocldClient,
  args: &VmArgs,
  name: &str,
) -> Result<(), CliError> {
  client.stop_vm(name, args.namespace.clone()).await?;

  Ok(())
}

pub async fn exec_vm_run(
  client: &NanocldClient,
  args: &VmArgs,
  options: &VmRunOpts,
) -> Result<(), CliError> {
  let vm = options.clone().into();
  let vm = client.create_vm(&vm, args.namespace.clone()).await?;
  client.start_vm(&vm.name, args.namespace.clone()).await?;

  Ok(())
}

pub async fn exec_vm_patch(
  client: &NanocldClient,
  args: &VmArgs,
  options: &VmPatchOpts,
) -> Result<(), CliError> {
  let vm = options.clone().into();
  client
    .patch_vm(&options.name, &vm, args.namespace.clone())
    .await?;

  Ok(())
}

pub async fn exec_vm(
  client: &NanocldClient,
  args: &VmArgs,
) -> Result<(), CliError> {
  match &args.commands {
    VmCommands::Image(args) => exec_vm_image(client, args).await,
    VmCommands::Create(options) => exec_vm_create(client, args, options).await,
    VmCommands::List => exec_vm_ls(client, args).await,
    VmCommands::Remove { names } => exec_vm_rm(client, args, names).await,
    VmCommands::Inspect { name } => exec_vm_inspect(client, args, name).await,
    VmCommands::Start { name } => exec_vm_start(client, args, name).await,
    VmCommands::Stop { name } => exec_vm_stop(client, args, name).await,
    VmCommands::Run(options) => exec_vm_run(client, args, options).await,
    VmCommands::Patch(options) => exec_vm_patch(client, args, options).await,
  }
}
