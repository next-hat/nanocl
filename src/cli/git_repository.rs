use indicatif::{ProgressBar, ProgressStyle};

use crate::client::Nanocld;
use crate::models::{
  GitRepositoryArgs, GitRepositoryCommands, GitRepositoryPartial,
  GitRepositoryDeleteOptions, GitRepositoryBuildOptions,
};

use super::errors::CliError;
use super::utils::print_table;

async fn exec_git_repository_list(client: &Nanocld) -> Result<(), CliError> {
  let items = client.list_git_repository().await?;
  print_table(items);
  Ok(())
}

async fn exec_git_repository_create(
  client: &Nanocld,
  item: &GitRepositoryPartial,
) -> Result<(), CliError> {
  client.create_git_repository(item).await?;
  println!("{}", item.name);
  Ok(())
}

async fn exec_git_repository_remove(
  client: &Nanocld,
  options: &GitRepositoryDeleteOptions,
) -> Result<(), CliError> {
  client
    .delete_git_repository(options.name.to_owned())
    .await?;
  Ok(())
}

async fn exec_git_repository_build(
  client: &Nanocld,
  options: &GitRepositoryBuildOptions,
) -> Result<(), CliError> {
  let pg = ProgressBar::new(0);
  let style = ProgressStyle::default_spinner();
  let mut is_new_action = false;
  pg.set_style(style);
  client
    .build_git_repository(options.name.to_owned(), |info| {
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
      if let Some(output) = info.stream {
        pg.println(output.trim());
      }
      if let Some(error) = info.error {
        eprintln!("{}", error);
      }
      pg.tick();
    })
    .await?;
  pg.finish_and_clear();
  Ok(())
}

pub async fn exec_git_repository(
  client: &Nanocld,
  args: &GitRepositoryArgs,
) -> Result<(), CliError> {
  match &args.commands {
    GitRepositoryCommands::List => exec_git_repository_list(client).await,
    GitRepositoryCommands::Create(item) => {
      exec_git_repository_create(client, item).await
    }
    GitRepositoryCommands::Remove(options) => {
      exec_git_repository_remove(client, options).await
    }
    GitRepositoryCommands::Build(options) => {
      exec_git_repository_build(client, options).await
    }
  }
}
