use std::io::BufRead;

use crate::client::Nanocld;
use crate::models::{NginxTemplateArgs, NginxTemplateCommand, NginxTemplatePartial};

use super::utils::print_table;
use super::errors::CliError;

pub async fn exec_nginx_template(
  client: &Nanocld,
  args: &NginxTemplateArgs,
) -> Result<(), CliError> {
  match &args.commands {
    NginxTemplateCommand::List => {
      let items = client.list_nginx_template().await?;
      print_table(items);
    }
    NginxTemplateCommand::Remove(options) => {
      client
        .delete_nginx_template(options.name.to_owned())
        .await?;
    }
    NginxTemplateCommand::Create(options) => {
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
        let item = NginxTemplatePartial {
          name: options.name.to_owned(),
          mode: options.mode.to_owned(),
          content,
        };
        let res = client.create_nginx_template(item).await?;
        println!("{}", &res.name);
      }
      if let Some(file_path) = &options.file_path {
        let content = std::fs::read_to_string(file_path)?;
        let item = NginxTemplatePartial {
          name: options.name.to_owned(),
          mode: options.mode.to_owned(),
          content,
        };
        let res = client.create_nginx_template(item).await?;
        println!("{}", &res.name);
      }
    }
  }
  Ok(())
}
