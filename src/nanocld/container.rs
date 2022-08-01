use std::collections::HashMap;
use clap::Parser;
use tabled::Tabled;
use futures::{TryStreamExt, StreamExt};
use serde::{Serialize, Deserialize};

use super::error::{NanocldError, is_api_error};
use super::models::{Port, EndpointSettings, optional_string};

use super::client::Nanocld;

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

/// A summary of the container's network settings
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ContainerSummaryNetworkSettings {
  #[serde(rename = "Networks")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub networks: Option<HashMap<String, EndpointSettings>>,
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

#[derive(Debug, Serialize, Deserialize)]
pub struct ContainerExecQuery {
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) attach_stdin: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) attach_stdout: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) attach_stderr: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) detach_keys: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) tty: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) env: Option<Vec<String>>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) cmd: Option<Vec<String>>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) privileged: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) user: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub(crate) working_dir: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ExecItem {
  #[serde(rename = "Id")]
  pub(crate) id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum LogOutputStreamTypes {
  StdErr,
  StdIn,
  StdOut,
  Console,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LogOutputStream {
  types: LogOutputStreamTypes,
  message: String,
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

  pub async fn create_exec(
    &self,
    name: &str,
    config: ContainerExecQuery,
  ) -> Result<ExecItem, NanocldError> {
    let mut res = self
      .post(format!("/containers/{}/exec", name))
      .send_json(&config)
      .await?;
    let status = res.status();

    is_api_error(&mut res, &status).await?;

    let exec = res.json::<ExecItem>().await?;

    Ok(exec)
  }

  pub async fn start_exec(&self, id: &str) -> Result<(), NanocldError> {
    let mut res = self.post(format!("/exec/{}/start", &id)).send().await?;
    let status = res.status();

    is_api_error(&mut res, &status).await?;

    let mut stream = res.into_stream();

    while let Some(output) = stream.next().await {
      match output {
        Err(err) => {
          eprintln!("{err}");
        }
        Ok(output) => {
          print!("{}", &String::from_utf8(output.to_vec()).unwrap());
        }
      }
    }
    Ok(())
  }
}
