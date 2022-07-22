use std::collections::HashMap;
use clap::Parser;
use serde::{Serialize, Deserialize};
use tabled::Tabled;

use super::error::{NanocldError, is_api_error};
use super::models::{Port, EndpointSettings, optional_string};

use super::client::Nanocld;

/// A summary of the container's network settings
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ContainerSummaryNetworkSettings {
  #[serde(rename = "Networks")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub networks: Option<HashMap<String, EndpointSettings>>,
}

fn optional_name(s: &Option<Vec<String>>) -> String {
  match s {
    None => String::from(""),
    Some(s) => s
      .iter()
      .map(|s| s.replace('/', ""))
      .collect::<Vec<_>>()
      .join(", "),
  }
}

fn display_optional_ports(s: &Option<Vec<Port>>) -> String {
  match s {
    None => String::from(""),
    Some(ports) => ports.iter().fold(String::new(), |mut acc, port| {
      acc = format!(
        "{}{}:{} ",
        acc,
        port.public_port.unwrap_or_default(),
        port.private_port
      );
      acc
    }),
  }
}

fn display_container_summary_network_settings(
  s: &Option<ContainerSummaryNetworkSettings>,
) -> String {
  match s {
    None => String::from(""),
    Some(summary) => {
      if let Some(network) = &summary.networks {
        let mut ips = String::new();
        for key in network.keys() {
          let netinfo = network.get(key).unwrap();
          let ip = netinfo.ip_address.to_owned().unwrap_or_default();
          ips = format!("{}{} ", ips, ip,);
        }
        return ips;
      }
      String::from("")
    }
  }
}

#[derive(Debug, Tabled, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ContainerSummary {
  /// The ID of this container
  #[serde(rename = "Id")]
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tabled(skip)]
  pub id: Option<String>,

  /// The names that this container has been given
  #[serde(rename = "Names")]
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tabled(display_with = "optional_name")]
  pub names: Option<Vec<String>>,

  /// The name of the image used when creating this container
  #[serde(rename = "Image")]
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tabled(display_with = "optional_string")]
  pub image: Option<String>,

  /// The ID of the image that this container was created from
  #[serde(rename = "ImageID")]
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tabled(skip)]
  pub image_id: Option<String>,

  /// Command to run when starting the container
  #[serde(rename = "Command")]
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tabled(skip)]
  pub command: Option<String>,

  /// When the container was created
  #[serde(rename = "Created")]
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tabled(skip)]
  pub created: Option<i64>,

  /// The ports exposed by this container
  #[serde(rename = "Ports")]
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tabled(display_with = "display_optional_ports")]
  pub ports: Option<Vec<Port>>,

  /// The size of files that have been created or changed by this container
  #[serde(rename = "SizeRw")]
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tabled(skip)]
  pub size_rw: Option<i64>,

  /// The total size of all the files in this container
  #[serde(rename = "SizeRootFs")]
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tabled(skip)]
  pub size_root_fs: Option<i64>,

  /// User-defined key/value metadata.
  #[serde(rename = "Labels")]
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tabled(skip)]
  pub labels: Option<HashMap<String, String>>,

  /// The state of this container (e.g. `Exited`)
  #[serde(rename = "State")]
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tabled(display_with = "optional_string")]
  pub state: Option<String>,

  /// Additional human-readable status of this container (e.g. `Exit 0`)
  #[serde(rename = "Status")]
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tabled(display_with = "optional_string")]
  pub status: Option<String>,

  #[serde(rename = "NetworkSettings")]
  #[serde(skip_serializing_if = "Option::is_none")]
  #[tabled(display_with = "display_container_summary_network_settings")]
  pub network_settings: Option<ContainerSummaryNetworkSettings>,
}

/// List container by namespace cluster or cargo
#[derive(Debug, Parser, Serialize, Deserialize)]
pub struct ListContainerOptions {
  /// Namespace where the container is stored
  #[clap(long)]
  namespace: Option<String>,
  /// Cluster where the container is stored
  #[clap(long)]
  cluster: Option<String>,
  /// Cargo where the container is stored
  #[clap(long)]
  cargo: Option<String>,
}

impl Nanocld {
  pub async fn list_containers(
    &self,
    options: &ListContainerOptions,
  ) -> Result<Vec<ContainerSummary>, NanocldError> {
    let mut res = self
      .get(String::from("/containers"))
      .query(options)
      .unwrap()
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    let data = res.json::<Vec<ContainerSummary>>().await?;

    Ok(data)
  }
}
