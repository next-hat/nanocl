use std::path::PathBuf;

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use ntex::http::StatusCode;

use crate::nanocld::cargo::CargoPartial;
use crate::nanocld::client::Nanocld;
use crate::nanocld::cluster::{
  ClusterNetworkPartial, ClusterPartial, ClusterVarPartial,
};

use crate::errors::CliError;
use crate::nanocld::error::NanocldError;

use super::parser::get_config_type;
use super::models::{YmlConfigTypes, NamespaceConfig};

async fn revert_namespace(
  namespace: &NamespaceConfig,
  client: &Nanocld,
) -> Result<(), CliError> {
  // Delete cargoes
  namespace
    .cargoes
    .iter()
    .map(|cargo| async {
      let result = client
        .delete_cargo(&cargo.name, Some(namespace.name.to_owned()))
        .await;
      if let Err(err) = result {
        match err {
          NanocldError::Api(ref api_err) => {
            if api_err.status == StatusCode::NOT_FOUND {
              return Ok::<(), CliError>(());
            }
            return Err::<(), CliError>(CliError::Client(err));
          }
          _ => {
            return Err::<(), CliError>(CliError::Client(err));
          }
        }
      }
      Ok::<(), CliError>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<()>, CliError>>()?;

  // Delete clusters
  namespace
    .clusters
    .iter()
    .map(|cluster| async {
      let result = client
        .delete_cluster(&cluster.name, Some(namespace.name.to_owned()))
        .await;
      if let Err(err) = result {
        match err {
          NanocldError::Api(ref api_err) => {
            if api_err.status == StatusCode::NOT_FOUND {
              return Ok::<(), CliError>(());
            }
            return Err::<(), CliError>(CliError::Client(err));
          }
          _ => {
            return Err::<(), CliError>(CliError::Client(err));
          }
        }
      }
      Ok::<(), CliError>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<()>, CliError>>()?;
  Ok(())
}

async fn apply_namespace(
  namespace: &NamespaceConfig,
  client: &Nanocld,
) -> Result<(), CliError> {
  // Create namespace if not exists
  if client.inspect_namespace(&namespace.name).await.is_err() {
    client.create_namespace(&namespace.name).await?;
  }

  // Create clusters
  namespace
    .clusters
    .iter()
    .map(|cluster| async {
      let cluster_exists = client
        .inspect_cluster(&cluster.name, Some(namespace.name.to_owned()))
        .await;
      let item = ClusterPartial {
        name: cluster.name.to_owned(),
        proxy_templates: cluster.proxy_templates.to_owned(),
      };
      if cluster_exists.is_err() {
        client
          .create_cluster(&item, Some(namespace.name.to_owned()))
          .await?;
      }
      // Create cluster variables
      if let Some(variables) = cluster.variables.to_owned() {
        let variables = &variables;
        variables
          .to_owned()
          .into_keys()
          .map(|var_name| async {
            let result = client
              .inspect_cluster_var(
                &cluster.name,
                &var_name,
                Some(namespace.name.to_owned()),
              )
              .await;
            let value = variables.get(&var_name).unwrap();
            let item = ClusterVarPartial {
              name: var_name,
              value: value.into(),
            };
            if result.is_err() {
              client
                .create_cluster_var(
                  &cluster.name.to_owned(),
                  &item,
                  Some(namespace.name.to_owned()),
                )
                .await?;
            }
            Ok::<_, CliError>(())
          })
          .collect::<FuturesUnordered<_>>()
          .collect::<Vec<_>>()
          .await
          .into_iter()
          .collect::<Result<Vec<()>, CliError>>()?;
      }
      // Create cluster networks
      namespace
        .networks
        .iter()
        .map(|network| async {
          let result = client
            .inspect_cluster_network(
              &cluster.name,
              &network.name,
              Some(namespace.name.to_owned()),
            )
            .await;
          let item = ClusterNetworkPartial {
            name: network.name.to_owned(),
          };
          if result.is_err() {
            client
              .create_cluster_network(
                &cluster.name,
                &item,
                Some(namespace.name.to_owned()),
              )
              .await?;
          }

          Ok::<_, CliError>(())
        })
        .collect::<FuturesUnordered<_>>()
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<()>, CliError>>()?;
      Ok::<_, CliError>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<()>, CliError>>()?;

  // Create cargoes
  namespace
    .cargoes
    .iter()
    .map(|cargo| async {
      let result = client
        .inspect_cargo(&cargo.name, Some(namespace.name.to_owned()))
        .await;
      let item = CargoPartial {
        name: cargo.name.to_owned(),
        dns_entry: cargo.dns_entry.to_owned(),
        image_name: cargo.image_name.to_owned(),
        binds: cargo.binds.to_owned(),
        replicas: cargo.replicas.to_owned(),
        environnements: cargo.environnements.to_owned(),
        domainname: cargo.domainname.to_owned(),
        hostname: cargo.hostname.to_owned(),
      };
      if result.is_err() {
        client
          .create_cargo(&item, Some(namespace.name.to_owned()))
          .await?;
      }
      Ok::<_, CliError>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<()>, CliError>>()?;

  namespace
    .clusters
    .iter()
    .map(|cluster| async {
      if let Some(joins) = &cluster.joins {
        joins
          .iter()
          .map(|join| async {
            if let Err(err) = client
              .join_cluster_cargo(
                &cluster.name,
                join,
                Some(namespace.name.to_owned()),
              )
              .await
            {
              if let NanocldError::Api(ref err) = err {
                if err.status == StatusCode::CONFLICT {
                  return Ok::<_, CliError>(());
                }
              }
              return Err(CliError::Client(err));
            }

            Ok::<_, CliError>(())
          })
          .collect::<FuturesUnordered<_>>()
          .collect::<Vec<_>>()
          .await
          .into_iter()
          .collect::<Result<Vec<()>, CliError>>()?;
      }

      if let Some(auto_start) = cluster.auto_start {
        if !auto_start {
          return Ok::<_, CliError>(());
        }
        client
          .start_cluster(&cluster.name, Some(namespace.name.to_owned()))
          .await?;
      }

      Ok::<_, CliError>(())
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<()>, CliError>>()?;

  Ok(())
}

pub async fn apply(
  file_path: PathBuf,
  client: &Nanocld,
) -> Result<(), CliError> {
  let file_content = std::fs::read_to_string(file_path)?;
  let config_type = get_config_type(&file_content)?;
  match config_type {
    YmlConfigTypes::Namespace => {
      let namespace = serde_yaml::from_str::<NamespaceConfig>(&file_content)?;
      apply_namespace(&namespace, client).await?;
    }
    _ => todo!("apply different type of config"),
  }
  Ok(())
}

pub async fn revert(
  file_path: PathBuf,
  client: &Nanocld,
) -> Result<(), CliError> {
  let file_content = std::fs::read_to_string(file_path)?;
  let config_type = get_config_type(&file_content)?;
  match config_type {
    YmlConfigTypes::Namespace => {
      let namespace = serde_yaml::from_str::<NamespaceConfig>(&file_content)?;
      revert_namespace(&namespace, client).await?;
    }
    _ => todo!("delete different type of config"),
  }
  Ok(())
}
