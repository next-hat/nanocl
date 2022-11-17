use std::io::BufRead;

use crate::client::Nanocld;
use crate::models::{
  ProxyArgs, ProxyCommands, ProxyTemplateArgs, ProxyTemplateCommand,
  ProxyTemplatePartial, ProxyLinkerOptions,
};

use super::errors::CliError;
use super::utils::print_table;

pub async fn exec_proxy_template(
  client: &Nanocld,
  args: &ProxyTemplateArgs,
) -> Result<(), CliError> {
  match &args.commands {
    ProxyTemplateCommand::List => {
      let items = client.list_proxy_template().await?;
      print_table(items);
    }
    ProxyTemplateCommand::Remove(options) => {
      client
        .delete_proxy_template(options.name.to_owned())
        .await?;
    }
    ProxyTemplateCommand::Create(options) => {
      if !options.is_reading_stdi && options.file_path.is_none() {
        eprintln!("Missing option use --help");
        std::process::exit(1);
      }
      if options.is_reading_stdi && options.file_path.is_some() {
        eprintln!("cannot have --stdi and -f options in same time.");
        std::process::exit(1);
      }
      if options.is_reading_stdi {
        let stdin = std::io::stdin();
        let mut content = String::new();
        let mut handle = stdin.lock();
        loop {
          let readed = handle.read_line(&mut content)?;
          if readed == 0 {
            break;
          }
        }
        let item = ProxyTemplatePartial {
          name: options.name.to_owned(),
          mode: options.mode.to_owned(),
          content,
        };
        let res = client.create_proxy_template(item).await?;
        println!("{}", &res.name);
      }
      if let Some(file_path) = &options.file_path {
        let content = std::fs::read_to_string(file_path)?;
        let item = ProxyTemplatePartial {
          name: options.name.to_owned(),
          mode: options.mode.to_owned(),
          content,
        };
        let res = client.create_proxy_template(item).await?;
        println!("{}", &res.name);
      }
    }
  }
  Ok(())
}

async fn exec_cluster_proxy_template_link(
  client: &Nanocld,
  options: &ProxyLinkerOptions,
) -> Result<(), CliError> {
  client
    .add_nginx_template_to_cluster(
      &options.cl_name,
      &options.nt_name,
      options.namespace.to_owned(),
    )
    .await?;
  Ok(())
}

async fn exec_cluster_proxy_template_unlink(
  client: &Nanocld,
  options: &ProxyLinkerOptions,
) -> Result<(), CliError> {
  client
    .remove_nginx_template_to_cluster(
      &options.cl_name,
      &options.nt_name,
      options.namespace.to_owned(),
    )
    .await?;
  Ok(())
}

pub async fn exec_proxy(
  client: &Nanocld,
  args: &ProxyArgs,
) -> Result<(), CliError> {
  match &args.commands {
    ProxyCommands::Template(template_args) => {
      exec_proxy_template(client, template_args).await
    }
    ProxyCommands::Link(opts) => {
      exec_cluster_proxy_template_link(client, opts).await
    }
    ProxyCommands::Unlink(opts) => {
      exec_cluster_proxy_template_unlink(client, opts).await
    }
  }
}
