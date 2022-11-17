use futures::StreamExt;
use indicatif::{ProgressStyle, ProgressBar};

use crate::client::Nanocld;
use crate::models::{
  CargoImageArgs, CargoImageCommands, CargoImageDeployOpts,
  CargoImageRemoveOpts, CargoImageInspectOpts,
};

use super::errors::CliError;
use super::utils::print_table;

async fn exec_cargo_instance_list(client: &Nanocld) -> Result<(), CliError> {
  let items = client.list_cargo_image().await?;
  print_table(items);
  Ok(())
}

async fn _exec_deploy_cargo_image(
  client: &Nanocld,
  options: &CargoImageDeployOpts,
) -> Result<(), CliError> {
  client._deploy_cargo_image(&options.name).await?;
  Ok(())
}

async fn exec_remove_cargo_image(
  client: &Nanocld,
  args: &CargoImageRemoveOpts,
) -> Result<(), CliError> {
  client.remove_cargo_image(&args.name).await?;
  Ok(())
}

pub async fn exec_create_cargo_image(
  client: &Nanocld,
  name: &str,
) -> Result<(), CliError> {
  let mut stream = client.create_cargo_image(name).await?;
  let style = ProgressStyle::default_spinner();
  let pg = ProgressBar::new(0);
  pg.set_style(style);
  let mut is_new_action = false;
  while let Some(info) = stream.next().await {
    let status = info.status.unwrap_or_default();
    let id = info.id.unwrap_or_default();
    match status.as_str() {
      "Downloading" => {
        if !is_new_action {
          is_new_action = true;
          pg.println(format!("{} {}", &status, &id).trim());
        }
      }
      "Extracting" => {
        if !is_new_action {
          is_new_action = true;
          pg.println(format!("{} {}", &status, &id).trim());
        } else {
        }
      }
      "Pull complete" => {
        is_new_action = false;
        pg.println(format!("{} {}", &status, &id).trim());
      }
      "Download complete" => {
        is_new_action = false;
        pg.println(format!("{} {}", &status, &id).trim());
      }
      _ => pg.println(format!("{} {}", &status, &id).trim()),
    };
    if let Some(error) = info.error {
      eprintln!("{}", error);
      break;
    }
    pg.tick();
  }
  pg.finish_and_clear();
  Ok(())
}

async fn exec_inspect_cargo_image(
  client: &Nanocld,
  opts: &CargoImageInspectOpts,
) -> Result<(), CliError> {
  let res = client.inspect_cargo_image(&opts.name).await?;
  print_table(vec![res]);
  Ok(())
}

pub async fn exec_cargo_image(
  client: &Nanocld,
  cmd: &CargoImageArgs,
) -> Result<(), CliError> {
  match &cmd.commands {
    CargoImageCommands::List => exec_cargo_instance_list(client).await,
    // CargoImageCommands::Deploy(options) => {
    //   exec_deploy_cargo_image(client, options).await
    // }
    CargoImageCommands::Inspect(opts) => {
      exec_inspect_cargo_image(client, opts).await
    }
    CargoImageCommands::Create(opts) => {
      exec_create_cargo_image(client, &opts.name).await
    }
    CargoImageCommands::Remove(args) => {
      exec_remove_cargo_image(client, args).await
    }
  }
}
