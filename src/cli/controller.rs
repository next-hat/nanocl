use crate::{
  models::{
    ControllerArgs, ControllerOptions, ControllerCommands, ControllerType,
    ClusterJoinPartial,
  },
  client::Nanocld,
};

use super::errors::CliError;

async fn add(
  client: &Nanocld,
  options: &ControllerOptions,
) -> Result<(), CliError> {
  let cluster = "nano";
  let namespace = Some(String::from("system"));
  match options.r#type {
    ControllerType::Proxy => {
      let join_options = ClusterJoinPartial {
        network: String::from("internal0"),
        cargo: String::from("nproxy"),
      };
      client
        .join_cluster_cargo(cluster, &join_options, namespace.to_owned())
        .await?;
    }
    ControllerType::Dns => {
      let join_options = ClusterJoinPartial {
        network: String::from("internal0"),
        cargo: String::from("ndns"),
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
      let name = "nproxy";
      client
        .delete_cargo_instance(name, cluster, namespace)
        .await?;
    }
    ControllerType::Dns => {
      let name = "ndns";
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
