mod cli;
mod yml;
mod models;
mod version;
mod client;
// #[cfg(feature = "genman")]
mod man;

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
    Commands::Docker(options) => {
      cli::exec_docker(options).await?;
    }
    Commands::Run(args) => {
      cli::exec_run(&client, args).await?;
    }
    Commands::Namespace(args) => {
      cli::exec_namespace(&client, args).await?;
    }
    Commands::Cluster(args) => {
      cli::exec_cluster(&client, args).await?;
    }
    Commands::Node(args) => match &args.subcommands {
      NodeCommands::Create(node) => {
        todo!("create node {:#?}", node);
      }
    },
    Commands::ListContainer(args) => {
      let data = client.list_containers(args).await?;
      print_table(data);
    }
    Commands::GitRepository(args) => {
      cli::exec_git_repository(&client, args).await?;
    }
    Commands::Cargo(args) => {
      cli::exec_cargo(&client, args).await?;
    }
    Commands::NginxTemplate(args) => {
      cli::exec_nginx_template(&client, args).await?;
    }
    Commands::Apply(args) => {
      let mut file_path = std::env::current_dir()?;
      file_path.push(&args.file_path);
      yml::config::apply(file_path, &client).await?;
    }
    Commands::Revert(args) => {
      let mut file_path = std::env::current_dir()?;
      file_path.push(&args.file_path);
      yml::config::revert(file_path, &client).await?;
    }
    Commands::ContainerImage(args) => {
      cli::exec_container_image(&client, args).await?;
    }
    Commands::NginxLog => {
      client.watch_nginx_logs().await?;
    }
    Commands::Version => {
      println!("=== [nanocli] ===");
      version::print_version();
      println!("=== [nanocld] ===");
      let daemon_version = client.get_version().await?;
      println!(
        "Arch: {}\nVersion: {}\nCommit ID: {}",
        daemon_version.arch, daemon_version.version, daemon_version.commit_id
      );
    }
    Commands::Exec(args) => {
      let config = ContainerExecQuery {
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
    }
  }
  Ok(())
}

#[ntex::main]
async fn main() -> std::io::Result<()> {
  #[cfg(feature = "genman")]
  {
    man::generate_man()?;
  }
  #[cfg(not(feature = "genman"))]
  {
    let args = Cli::parse();
    if let Err(err) = execute_args(&args).await {
      process_error(&args, err);
    }
  }
  Ok(())
}
