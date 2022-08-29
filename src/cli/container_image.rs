use futures::StreamExt;
use indicatif::{ProgressStyle, ProgressBar};

use crate::client::Nanocld;
use crate::models::{
  ContainerImageArgs, ContainerImageCommands, ContainerImageDeployOpts,
  ContainerImageRemoveOpts,
};

use super::errors::CliError;
use super::utils::print_table;

async fn exec_container_list(client: &Nanocld) -> Result<(), CliError> {
  let items = client.list_container_image().await?;
  print_table(items);
  Ok(())
}

async fn exec_deploy_container_image(
  client: &Nanocld,
  options: &ContainerImageDeployOpts,
) -> Result<(), CliError> {
  client.deploy_container_image(&options.name).await?;
  Ok(())
}

async fn exec_remove_container_image(
  client: &Nanocld,
  args: &ContainerImageRemoveOpts,
) -> Result<(), CliError> {
  client.remove_container_image(&args.name).await?;
  Ok(())
}

pub async fn exec_create_container_image(
  client: &Nanocld,
  name: &str,
) -> Result<(), CliError> {
  let mut stream = client.create_container_image(name).await?;
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

pub async fn exec_container_image(
  client: &Nanocld,
  cmd: &ContainerImageArgs,
) -> Result<(), CliError> {
  match &cmd.commands {
    ContainerImageCommands::List => exec_container_list(client).await,
    ContainerImageCommands::Deploy(options) => {
      exec_deploy_container_image(client, options).await
    }
    ContainerImageCommands::Create(options) => {
      exec_create_container_image(client, &options.name).await
    }
    ContainerImageCommands::Remove(args) => {
      exec_remove_container_image(client, args).await
    }
  }
}
