mod cli;
mod models;
mod version;
mod client;
mod utils;
mod config;

use clap::Parser;
use cli::errors::CliError;

use models::*;
use cli::utils::print_table;

fn process_error(args: &Cli, err: CliError) {
  match err {
    CliError::Client(err) => match err {
      client::error::NanocldError::SendRequest(err) => match err {
        ntex::http::client::error::SendRequestError::Connect(_) => {
          eprintln!(
            "Cannot connect to the nanocl daemon at {host}. Is the nanocl daemon running?",
            host = args.host
          )
        }
        _ => eprintln!("{}", err),
      },
      client::error::NanocldError::Api(err) => {
        eprintln!("Daemon [{}]: {}", err.status, err.msg);
      }
      _ => eprintln!("{}", err),
    },
    _ => eprintln!("{}", err),
  }
  std::process::exit(1);
}

async fn execute_args(args: &Cli) -> Result<(), CliError> {
  let client = client::Nanocld::connect_with_unix_default().await;
  match &args.command {
    Commands::Setup(args) => cli::exec_setup(args).await,
    Commands::Run(args) => cli::exec_run(&client, args).await,
    Commands::Namespace(args) => cli::exec_namespace(&client, args).await,
    Commands::Cluster(args) => cli::exec_cluster(&client, args).await,
    Commands::GitRepository(args) => {
      cli::exec_git_repository(&client, args).await
    }
    Commands::Cargo(args) => cli::exec_cargo(&client, args).await,
    Commands::NginxTemplate(args) => {
      cli::exec_nginx_template(&client, args).await
    }
    Commands::ContainerImage(args) => {
      cli::exec_container_image(&client, args).await
    }
    Commands::Version => cli::exec_version(&client).await,
    Commands::Exec(args) => cli::exec_exec(&client, args).await,
    Commands::Apply(args) => cli::exec_apply(&client, args).await,
    Commands::Revert(args) => cli::exec_revert(&client, args).await,
    Commands::ListContainer(args) => {
      let data = client.list_containers(args).await?;
      print_table(data);
      Ok(())
    }
    Commands::Controller(args) => cli::exec_controller(&client, args).await,
  }
}

#[ntex::main]
async fn main() -> std::io::Result<()> {
  let args = Cli::parse();
  if let Err(err) = execute_args(&args).await {
    process_error(&args, err);
  }
  Ok(())
}
