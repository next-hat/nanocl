use crate::{
  models::{
    ControllerArgs, ControllerOptions, ControllerCommands, ControllerType,
    ClusterJoinPartial,
  },
  client::Nanocld,
  config,
  utils::cargo_image,
};

use super::errors::CliError;

async fn add(
  client: &Nanocld,
  options: &ControllerOptions,
) -> Result<(), CliError> {
  let cluster = "nano";
  let namespace = Some(String::from("system"));
  let config = config::read_daemon_config_file(&String::from("/etc/nanocl"))?;
  // Connect to docker daemon
  let docker_api = bollard::Docker::connect_with_unix(
    &config.docker_host,
    120,
    bollard::API_DEFAULT_VERSION,
  )?;
  match options.r#type {
    ControllerType::Proxy => {
      let proxy_image_url = "https://github.com/nxthat/nanocl-ctrl-proxy/releases/download/v0.0.1/nanocl-proxy.0.0.1.tar.gz";
      cargo_image::import_tar_from_url(&docker_api, &proxy_image_url).await?;

      let join_options = ClusterJoinPartial {
        network: String::from("internal0"),
        cargo: String::from("proxy"),
      };
      client
        .join_cluster_cargo(cluster, &join_options, namespace.to_owned())
        .await?;
    }
    ControllerType::Dns => {
      let dns_image_url = "https://github.com/nxthat/nanocl-ctrl-dns/releases/download/v0.0.2/nanocl-dns.0.0.2.tar.gz";
      cargo_image::import_tar_from_url(&docker_api, &dns_image_url).await?;
      let join_options = ClusterJoinPartial {
        network: String::from("internal0"),
        cargo: String::from("dns"),
      };
      client
        .join_cluster_cargo(cluster, &join_options, namespace.to_owned())
        .await?;
    }
    ControllerType::Vpn => todo!("Vpn controller is not implemented yet."),
  }
  client.start_cluster(cluster, namespace.to_owned()).await?;
  Ok(())
}

async fn remove(
  client: &Nanocld,
  options: &ControllerOptions,
) -> Result<(), CliError> {
  let cluster = "nano";
  let namespace = Some(String::from("system"));

  match options.r#type {
    ControllerType::Proxy => {
      let name = "proxy";
      client
        .delete_cargo_instance(name, cluster, namespace)
        .await?;
    }
    ControllerType::Dns => {
      let name = "dns";
      client
        .delete_cargo_instance(name, cluster, namespace)
        .await?;
    }
    ControllerType::Vpn => todo!("Vpn controller is not implemented yet."),
  }

  // client.delete_cargo_instance()
  Ok(())
}

pub async fn exec_controller(
  client: &Nanocld,
  args: &ControllerArgs,
) -> Result<(), CliError> {
  match &args.commands {
    ControllerCommands::Add(options) => add(client, options).await,
    ControllerCommands::Remove(options) => remove(client, options).await,
  }
}
