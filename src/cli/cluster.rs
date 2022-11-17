use crate::client::Nanocld;
use crate::models::{
  ClusterArgs, ClusterCommands, ClusterPartial, ClusterDeleteOptions,
  ClusterStartOptions, ClusterInspectOptions, ClusterJoinOptions,
  ClusterJoinPartial, ClusterNginxTemplateCommands, ClusterNginxTemplateArgs,
  ClusterNginxTemplateCommandsOption, ClusterNetworkArgs,
  ClusterNetworkPartial, ClusterNetworkCommands, ClusterNetworkDeleteOptions,
  ClusterVariableArgs, ClusterVariableCommands, ClusterVarPartial,
  ClusterVariableRemoveOptions,
};

use super::errors::CliError;
use super::utils::print_table;

async fn exec_cluster_list(
  client: &Nanocld,
  args: &ClusterArgs,
) -> Result<(), CliError> {
  let items = client.list_cluster(args.namespace.to_owned()).await?;
  print_table(items);
  Ok(())
}

async fn exec_cluster_create(
  client: &Nanocld,
  args: &ClusterArgs,
  item: &ClusterPartial,
) -> Result<(), CliError> {
  let item = client
    .create_cluster(item, args.namespace.to_owned())
    .await?;
  println!("{}", item.key);
  Ok(())
}

async fn exec_cluster_remove(
  client: &Nanocld,
  args: &ClusterArgs,
  options: &ClusterDeleteOptions,
) -> Result<(), CliError> {
  client
    .delete_cluster(&options.name, args.namespace.to_owned())
    .await?;
  Ok(())
}

async fn exec_cluster_start(
  client: &Nanocld,
  args: &ClusterArgs,
  options: &ClusterStartOptions,
) -> Result<(), CliError> {
  client
    .start_cluster(&options.name, args.namespace.to_owned())
    .await?;
  Ok(())
}

async fn exec_cluster_inspect(
  client: &Nanocld,
  args: &ClusterArgs,
  options: &ClusterInspectOptions,
) -> Result<(), CliError> {
  let cluster = client
    .inspect_cluster(&options.name, args.namespace.to_owned())
    .await?;
  println!("> CLUSTER");
  print_table(vec![&cluster]);
  println!("\n> VARIABLES");
  print_table(cluster.variables);
  println!("\n> NETWORKS");
  print_table(cluster.networks.unwrap_or_default());
  println!("\n> CARGOES");
  print_table(cluster.cargoes.unwrap_or_default());
  Ok(())
}

async fn exec_cluster_join(
  client: &Nanocld,
  args: &ClusterArgs,
  options: &ClusterJoinOptions,
) -> Result<(), CliError> {
  let join_opts = ClusterJoinPartial {
    network: options.network_name.to_owned(),
    cargo: options.cargo_name.to_owned(),
  };
  client
    .join_cluster_cargo(
      &options.cluster_name,
      &join_opts,
      args.namespace.to_owned(),
    )
    .await?;
  Ok(())
}

async fn exec_cluster_nginx_template_add(
  client: &Nanocld,
  args: &ClusterArgs,
  options: &ClusterNginxTemplateCommandsOption,
) -> Result<(), CliError> {
  client
    .add_nginx_template_to_cluster(
      &options.cl_name,
      &options.nt_name,
      args.namespace.to_owned(),
    )
    .await?;
  Ok(())
}

async fn exec_cluster_nginx_template_remove(
  client: &Nanocld,
  args: &ClusterArgs,
  options: &ClusterNginxTemplateCommandsOption,
) -> Result<(), CliError> {
  client
    .remove_nginx_template_to_cluster(
      &options.cl_name,
      &options.nt_name,
      args.namespace.to_owned(),
    )
    .await?;
  Ok(())
}

async fn exec_cluster_nginx_template(
  client: &Nanocld,
  args: &ClusterArgs,
  ntargs: &ClusterNginxTemplateArgs,
) -> Result<(), CliError> {
  match &ntargs.commands {
    ClusterNginxTemplateCommands::Add(options) => {
      exec_cluster_nginx_template_add(client, args, options).await
    }
    ClusterNginxTemplateCommands::Remove(options) => {
      exec_cluster_nginx_template_remove(client, args, options).await
    }
  }
}

async fn exec_cluster_network_list(
  client: &Nanocld,
  args: &ClusterArgs,
  nargs: &ClusterNetworkArgs,
) -> Result<(), CliError> {
  let items = client
    .list_cluster_network(&nargs.cluster, args.namespace.to_owned())
    .await?;
  print_table(items);
  Ok(())
}

async fn exec_cluster_network_create(
  client: &Nanocld,
  args: &ClusterArgs,
  nargs: &ClusterNetworkArgs,
  item: &ClusterNetworkPartial,
) -> Result<(), CliError> {
  let item = client
    .create_cluster_network(&nargs.cluster, item, args.namespace.to_owned())
    .await?;
  println!("{}", item.key);
  Ok(())
}

async fn exec_cluster_network_remove(
  client: &Nanocld,
  args: &ClusterArgs,
  nargs: &ClusterNetworkArgs,
  options: &ClusterNetworkDeleteOptions,
) -> Result<(), CliError> {
  client
    .delete_cluster_network(
      &nargs.cluster,
      &options.name,
      args.namespace.to_owned(),
    )
    .await?;
  Ok(())
}

async fn exec_cluster_network(
  client: &Nanocld,
  args: &ClusterArgs,
  nargs: &ClusterNetworkArgs,
) -> Result<(), CliError> {
  match &nargs.commands {
    ClusterNetworkCommands::List => {
      exec_cluster_network_list(client, args, nargs).await
    }
    ClusterNetworkCommands::Create(item) => {
      exec_cluster_network_create(client, args, nargs, item).await
    }
    ClusterNetworkCommands::Remove(options) => {
      exec_cluster_network_remove(client, args, nargs, options).await
    }
  }
}

async fn exec_cluster_variable_create(
  client: &Nanocld,
  args: &ClusterArgs,
  vargs: &ClusterVariableArgs,
  item: &ClusterVarPartial,
) -> Result<(), CliError> {
  client
    .create_cluster_var(&vargs.cluster, item, args.namespace.to_owned())
    .await?;
  Ok(())
}

async fn exec_cluster_variable_remove(
  client: &Nanocld,
  args: &ClusterArgs,
  vargs: &ClusterVariableArgs,
  options: &ClusterVariableRemoveOptions,
) -> Result<(), CliError> {
  client
    .delete_cluster_var(
      &vargs.cluster,
      &options.name,
      args.namespace.to_owned(),
    )
    .await?;
  Ok(())
}

async fn exec_cluster_variable(
  client: &Nanocld,
  args: &ClusterArgs,
  vargs: &ClusterVariableArgs,
) -> Result<(), CliError> {
  match &vargs.commands {
    ClusterVariableCommands::Create(item) => {
      exec_cluster_variable_create(client, args, vargs, item).await
    }
    ClusterVariableCommands::Remove(options) => {
      exec_cluster_variable_remove(client, args, vargs, options).await
    }
  }
}

pub async fn exec_cluster(
  client: &Nanocld,
  args: &ClusterArgs,
) -> Result<(), CliError> {
  match &args.commands {
    ClusterCommands::List => exec_cluster_list(client, args).await,
    ClusterCommands::Create(item) => {
      exec_cluster_create(client, args, item).await
    }
    ClusterCommands::Remove(options) => {
      exec_cluster_remove(client, args, options).await
    }
    ClusterCommands::Start(options) => {
      exec_cluster_start(client, args, options).await
    }
    ClusterCommands::Inspect(options) => {
      exec_cluster_inspect(client, args, options).await
    }
    ClusterCommands::Join(options) => {
      exec_cluster_join(client, args, options).await
    }
    ClusterCommands::NginxTemplate(ntargs) => {
      exec_cluster_nginx_template(client, args, ntargs).await
    }
    ClusterCommands::Network(nargs) => {
      exec_cluster_network(client, args, nargs).await
    }
    ClusterCommands::Variable(vargs) => {
      exec_cluster_variable(client, args, vargs).await
    }
  }
}
