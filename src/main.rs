use clap::Parser;
use errors::CliError;
use futures::StreamExt;
use futures::stream::FuturesUnordered;
use indicatif::{ProgressBar, ProgressStyle};
use nanocld::{
  nginx_template::NginxTemplatePartial,
  cluster::{ClusterPartial, ClusterNetworkPartial, ClusterJoinPartial},
  cargo::CargoPartial,
  error::NanocldError,
  client::Nanocld,
  container::ContainerExecQuery,
};
use ntex::http::StatusCode;
use serde::{Serialize, Deserialize};

use std::{
  process::{Command, Stdio},
  io::BufRead,
};

use tabled::{
  object::{Segment, Rows},
  Padding, Alignment, Table, Style, Modify, Tabled,
};

mod cli;
mod yml;
mod errors;
mod version;
mod nanocld;
// #[cfg(feature = "genman")]
mod man;

use cli::*;

fn process_error(args: &Cli, err: errors::CliError) {
  match err {
    CliError::Client(err) => match err {
      nanocld::error::NanocldError::SendRequest(err) => match err {
        ntex::http::client::error::SendRequestError::Connect(_) => {
          eprintln!(
            "Cannot connect to the nanocl daemon at {host}. Is the nanocl daemon running?",
            host = args.host
          )
        }
        _ => eprintln!("{}", err),
      },
      nanocld::error::NanocldError::Api(err) => {
        eprintln!("Daemon [{}]: {}", err.status, err.msg);
      }
      _ => eprintln!("{}", err),
    },
    _ => eprintln!("{}", err),
  }
  std::process::exit(1);
}

fn print_table<T>(iter: impl IntoIterator<Item = T>)
where
  T: tabled::Tabled,
{
  let table = Table::new(iter)
    .with(Style::empty())
    .with(
      Modify::new(Segment::all())
        .with(Padding::new(0, 4, 0, 0))
        .with(Alignment::left()),
    )
    .with(Modify::new(Rows::first()).with(str::to_uppercase))
    .to_string();
  print!("{}", table);
}

#[derive(Debug, Tabled, Serialize, Deserialize)]
pub struct NamespaceWithCount {
  name: String,
  cargoes: usize,
  clusters: usize,
  networks: usize,
}

async fn execute_create_container_image(
  name: &str,
  client: &Nanocld,
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

async fn execute_args(args: &Cli) -> Result<(), CliError> {
  let client = nanocld::client::Nanocld::connect_with_unix_default().await;
  match &args.command {
    Commands::Docker(options) => {
      let mut opts = vec![
        String::from("-H"),
        String::from("unix:///run/nanocl/docker.sock"),
      ];
      let mut more_options = options.args.clone();
      opts.append(&mut more_options);

      let mut cmd = Command::new("docker")
        .args(&opts)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap();

      let _status = cmd.wait();
    }
    Commands::Node(args) => match &args.subcommands {
      NodeCommands::Create => {}
    },
    Commands::ListContainer(args) => {
      let data = client.list_containers(args).await?;
      print_table(data);
    }
    Commands::Run(args) => {
      if let Err(_err) = client.inspect_image(&args.image).await {
        execute_create_container_image(&args.image, &client).await?;
      }

      let cluster = ClusterPartial {
        name: args.cluster.to_owned(),
        proxy_templates: None,
      };
      if let Err(err) = client
        .create_cluster(&cluster, args.namespace.to_owned())
        .await
      {
        if let NanocldError::Api(err) = err {
          if err.status != StatusCode::CONFLICT {
            return Err(CliError::Client(nanocld::error::NanocldError::Api(
              err,
            )));
          }
        } else {
          return Err(CliError::Client(err));
        }
      }
      let cluster_network = ClusterNetworkPartial {
        name: args.network.to_owned(),
      };
      if let Err(err) = client
        .create_cluster_network(
          &args.cluster,
          &cluster_network,
          args.namespace.to_owned(),
        )
        .await
      {
        if let NanocldError::Api(err) = err {
          if err.status != StatusCode::CONFLICT {
            return Err(CliError::Client(nanocld::error::NanocldError::Api(
              err,
            )));
          }
        } else {
          return Err(CliError::Client(err));
        }
      }

      let cargo = CargoPartial {
        name: args.name.to_owned(),
        image_name: args.image.to_owned(),
        binds: None,
        replicas: None,
        dns_entry: None,
        domainname: None,
        hostname: None,
        environnements: None,
      };
      if let Err(err) =
        client.create_cargo(&cargo, args.namespace.to_owned()).await
      {
        if let NanocldError::Api(err) = err {
          if err.status != StatusCode::CONFLICT {
            return Err(CliError::Client(nanocld::error::NanocldError::Api(
              err,
            )));
          }
        } else {
          return Err(CliError::Client(err));
        }
      }

      let cluster_join = ClusterJoinPartial {
        network: args.network.to_owned(),
        cargo: args.name.to_owned(),
      };
      client
        .join_cluster_cargo(
          &args.cluster,
          &cluster_join,
          args.namespace.to_owned(),
        )
        .await?;
      client
        .start_cluster(&args.cluster, args.namespace.to_owned())
        .await?;
    }
    Commands::Namespace(args) => match &args.commands {
      NamespaceCommands::List => {
        let items = client.list_namespace().await?;
        let namespaces = items
          .iter()
          .map(|item| async {
            let cargo_count =
              client.count_cargo(Some(item.name.to_owned())).await?;
            let cluster_count =
              client.count_cluster(Some(item.name.to_owned())).await?;
            let network_count = client
              .count_cluster_network_by_nsp(Some(item.name.to_owned()))
              .await?;
            let new_item = NamespaceWithCount {
              name: item.name.to_owned(),
              cargoes: cargo_count.count,
              clusters: cluster_count.count,
              networks: network_count.count,
            };
            Ok::<_, CliError>(new_item)
          })
          .collect::<FuturesUnordered<_>>()
          .collect::<Vec<_>>()
          .await
          .into_iter()
          .collect::<Result<Vec<NamespaceWithCount>, CliError>>()?;

        print_table(namespaces);
      }
      NamespaceCommands::Create(item) => {
        let item = client.create_namespace(&item.name).await?;
        println!("{}", item.name);
      }
    },
    Commands::Cluster(args) => match &args.commands {
      ClusterCommands::List => {
        let items = client.list_cluster(args.namespace.to_owned()).await?;
        print_table(items);
      }
      ClusterCommands::Create(item) => {
        let item = client
          .create_cluster(item, args.namespace.to_owned())
          .await?;
        println!("{}", item.key);
      }
      ClusterCommands::Remove(options) => {
        client
          .delete_cluster(&options.name, args.namespace.to_owned())
          .await?;
      }
      ClusterCommands::Start(options) => {
        client
          .start_cluster(&options.name, args.namespace.to_owned())
          .await?;
      }
      ClusterCommands::Inspect(options) => {
        let cluster = client
          .inspect_cluster(&options.name, args.namespace.to_owned())
          .await?;
        println!("\n> CLUSTER");
        print_table(vec![&cluster]);
        println!("\n> VARIABLES");
        print_table(cluster.variables);
        println!("\n> NETWORKS");
        print_table(cluster.networks.unwrap_or_default());
        println!("\n> CARGOES");
        print_table(cluster.cargoes.unwrap_or_default());
      }
      ClusterCommands::Join(join_args) => {
        let join_opts = ClusterJoinPartial {
          network: join_args.network_name.to_owned(),
          cargo: join_args.cargo_name.to_owned(),
        };
        client
          .join_cluster_cargo(
            &join_args.cluster_name,
            &join_opts,
            args.namespace.to_owned(),
          )
          .await?;
      }
      ClusterCommands::NginxTemplate(nt_args) => match &nt_args.commands {
        ClusterNginxTemplateCommands::Add(nt_add_opts) => {
          client
            .add_nginx_template_to_cluster(
              &nt_add_opts.cl_name,
              &nt_add_opts.nt_name,
              args.namespace.to_owned(),
            )
            .await?;
        }
        ClusterNginxTemplateCommands::Remove(nt_rm_opts) => {
          client
            .remove_nginx_template_to_cluster(
              &nt_rm_opts.cl_name,
              &nt_rm_opts.nt_name,
              args.namespace.to_owned(),
            )
            .await?;
        }
      },
      ClusterCommands::Network(network_opts) => match &network_opts.commands {
        ClusterNetworkCommands::List => {
          let items = client
            .list_cluster_network(
              &network_opts.cluster,
              args.namespace.to_owned(),
            )
            .await?;
          print_table(items);
        }
        ClusterNetworkCommands::Create(item) => {
          let item = client
            .create_cluster_network(
              &network_opts.cluster,
              item,
              args.namespace.to_owned(),
            )
            .await?;
          println!("{}", item.key);
        }
        ClusterNetworkCommands::Remove(options) => {
          client
            .delete_cluster_network(
              &network_opts.cluster,
              &options.name,
              args.namespace.to_owned(),
            )
            .await?;
        }
      },
      ClusterCommands::Variable(var_opts) => match &var_opts.commands {
        ClusterVariableCommands::Create(var) => {
          client
            .create_cluster_var(
              &var_opts.cluster,
              var,
              args.namespace.to_owned(),
            )
            .await?;
        }
        ClusterVariableCommands::Remove(rm_args) => {
          client
            .delete_cluster_var(
              &var_opts.cluster,
              &rm_args.name,
              args.namespace.to_owned(),
            )
            .await?;
        }
      },
    },
    Commands::GitRepository(args) => match &args.commands {
      GitRepositoryCommands::List => {
        let items = client.list_git_repository().await?;
        print_table(items);
      }
      GitRepositoryCommands::Create(item) => {
        client.create_git_repository(item).await?;
        println!("{}", item.name);
      }
      GitRepositoryCommands::Remove(options) => {
        client
          .delete_git_repository(options.name.to_owned())
          .await?;
      }
      GitRepositoryCommands::Build(options) => {
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
      }
    },
    Commands::Cargo(args) => match &args.commands {
      CargoCommands::List => {
        let items = client.list_cargo(args.namespace.to_owned()).await?;
        print_table(items);
      }
      CargoCommands::Create(item) => {
        let item = client.create_cargo(item, args.namespace.to_owned()).await?;
        println!("{}", item.key);
      }
      CargoCommands::Remove(options) => {
        client
          .delete_cargo(&options.name, args.namespace.to_owned())
          .await?;
      }
      CargoCommands::Inspect(options) => {
        let cargo = client
          .inspect_cargo(&options.name, args.namespace.to_owned())
          .await?;

        println!("\n> CARGO");
        print_table([&cargo]);
        println!("\n> CONTAINERS");
        print_table(cargo.containers);
      }
      CargoCommands::Patch(opts) => match &opts.commands {
        CargoPatchCommands::Set(p) => {
          let cargo = client
            .update_cargo(&opts.name, args.namespace.to_owned(), p)
            .await?;
          println!("{:#?}", cargo);
        }
      },
    },
    Commands::NginxTemplate(args) => match &args.commands {
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
    },
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
    Commands::ContainerImage(args) => match &args.commands {
      ContainerImageCommands::List => {
        let items = client.list_container_image().await?;
        print_table(items);
      }
      ContainerImageCommands::Deploy(options) => {
        client.deploy_container_image(&options.name).await?;
      }
      ContainerImageCommands::Create(options) => {
        execute_create_container_image(&options.name, &client).await?;
      }
      ContainerImageCommands::Remove(args) => {
        client.remove_container_image(&args.name).await?;
      }
    },
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
