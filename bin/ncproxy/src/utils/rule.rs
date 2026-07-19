use std::{collections::BTreeSet, net::IpAddr};

use nanocl_error::io::{FromIo, IoError, IoResult};

use nanocld_client::{
  NanocldClient,
  stubs::{
    generic::NetworkKind,
    network::{Network, gen_network_key},
    process::Process,
    proxy::{
      Hsts, HstsConfig, ProxySsl, ProxySslConfig, StreamTarget, UnixTarget,
      UpstreamTarget,
    },
  },
};

use crate::models::{
  SystemStateRef, UNIX_UPSTREAM_TEMPLATE, UPSTREAM_TEMPLATE,
};

pub(crate) struct PreparedFile {
  pub(crate) path: String,
  pub(crate) data: String,
}

pub(crate) struct PreparedSsl {
  pub(crate) config: ProxySslConfig,
  pub(crate) files: Vec<PreparedFile>,
}

pub(crate) struct PreparedUpstream {
  pub(crate) key: String,
  pub(crate) content: String,
}

/// Get public address of host
async fn get_host_addr(client: &NanocldClient) -> IoResult<String> {
  let info = client
    .info()
    .await
    .map_err(|err| err.map_err_context(|| "Unable to get host info"))?;
  Ok(info.host_gateway)
}

/// Resolve the node that owns bridge networks visible to this controller.
/// Installed controllers receive NANOCL_NODE explicitly. The Docker daemon
/// name returned by `/info` keeps development and upgraded installations
/// working when that environment variable is absent.
async fn get_local_node(client: &NanocldClient) -> IoResult<String> {
  if let Ok(node) = std::env::var("NANOCL_NODE")
    && !node.trim().is_empty()
  {
    return Ok(node);
  }
  let info = client
    .info()
    .await
    .map_err(|err| err.map_err_context(|| "Unable to get local node info"))?;
  if !info.config.hostname.trim().is_empty() {
    return Ok(info.config.hostname);
  }
  info
    .docker
    .name
    .filter(|name| !name.trim().is_empty())
    .ok_or_else(|| {
      IoError::invalid_data(
        "Network",
        "NANOCL_NODE is unset and daemon info has no node name",
      )
    })
}

fn network_gateway(network: &Network) -> IoResult<String> {
  network
    .data
    .ipam
    .as_ref()
    .and_then(|ipam| ipam.config.as_ref())
    .and_then(|configs| {
      configs
        .iter()
        .filter_map(|config| config.gateway.as_deref())
        .find(|gateway| !gateway.is_empty())
    })
    .map(str::to_owned)
    .ok_or_else(|| {
      IoError::invalid_data(
        "Network",
        &format!("Network {} has no IPAM gateway", network.name),
      )
    })
}

async fn get_named_network_addr(
  name: &str,
  client: &NanocldClient,
) -> IoResult<String> {
  let node_name = get_local_node(client).await?;
  let key = gen_network_key(&node_name, name);
  let network = client.inspect_network(&key).await.map_err(|err| {
    err.map_err_context(|| {
      format!("Unable to inspect network {name} on node {node_name}")
    })
  })?;
  network_gateway(&network)
}

/// Get address of nanoclbr0 network
async fn get_bridge_addr(client: &NanocldClient) -> IoResult<String> {
  let info = client
    .info()
    .await
    .map_err(|err| err.map_err_context(|| "Unable to get host info"))?;
  let ipam = info.network.ipam.unwrap_or_default();
  let ipam_config = ipam.config.unwrap_or_default();
  let Some(network) = ipam_config.first() else {
    return Err(IoError::invalid_data(
      "Network",
      "No network found for nanoclbr0",
    ));
  };
  Ok(network.gateway.clone().unwrap_or_default())
}

fn format_listen_addr(ip: &str, port: u16) -> IoResult<String> {
  let ip = ip.parse::<IpAddr>().map_err(|err| {
    IoError::invalid_data(
      "Network",
      &format!("Invalid listener address {ip}: {err}"),
    )
  })?;
  Ok(std::net::SocketAddr::new(ip, port).to_string())
}

fn parse_upstream_target(key: &str) -> IoResult<(String, String, String)> {
  let info = key.split('.').collect::<Vec<&str>>();
  if info.len() < 3 {
    return Err(IoError::invalid_data(
      "TargetKey",
      "Invalid expected <name>.<namespace>.<kind>",
    ));
  }
  let name = info[0].to_owned();
  let namespace = info[1].to_owned();
  let kind = info[2].to_owned();
  Ok((name, namespace, kind))
}

fn process_is_running(process: &Process) -> bool {
  process
    .data
    .state
    .as_ref()
    .and_then(|state| state.status.as_ref())
    .is_some_and(|status| status.to_string() == "running")
}

fn process_network_address(process: &Process) -> Option<String> {
  let host_config = process.data.host_config.as_ref();
  let mode = host_config
    .and_then(|config| config.network_mode.as_deref())
    .unwrap_or("nanoclbr0");

  if mode == "host" {
    return Some("127.0.0.1".to_owned());
  }
  if mode == "none" || mode.starts_with("container:") {
    return None;
  }

  let networks = process
    .data
    .network_settings
    .as_ref()
    .and_then(|settings| settings.networks.as_ref())?;
  let endpoint = match mode {
    "" | "default" => {
      networks.get("nanoclbr0").or_else(|| networks.get("bridge"))
    }
    "bridge" => networks.get("bridge"),
    mode => networks.get(mode),
  }?;
  endpoint
    .ip_address
    .as_ref()
    .filter(|address| !address.is_empty())
    .cloned()
}

pub async fn get_addresses(
  processes: &[Process],
  local_node: &str,
) -> IoResult<Vec<String>> {
  let mut addresses = BTreeSet::new();
  for process in processes {
    log::debug!("get_addresses from: {}", process.name);
    if process.name.starts_with("tmp-")
      || process.name.starts_with("init-")
      || process.node_name != local_node
      || !process_is_running(process)
    {
      continue;
    }
    if let Some(address) = process_network_address(process) {
      addresses.insert(address);
    }
  }
  if addresses.is_empty() {
    return Err(IoError::invalid_data(
      "Process",
      &format!("No running process address found on local node {local_node}"),
    ));
  }
  Ok(addresses.into_iter().collect())
}

pub async fn get_network_addr(
  network: &NetworkKind,
  port: u16,
  client: &NanocldClient,
) -> IoResult<String> {
  match network {
    NetworkKind::All => Ok(format!("{port}")),
    NetworkKind::Public => {
      let ip = get_host_addr(client).await?;
      format_listen_addr(&ip, port)
    }
    NetworkKind::Local => format_listen_addr("127.0.0.1", port),
    NetworkKind::Internal => {
      let ip = get_bridge_addr(client).await?;
      format_listen_addr(&ip, port)
    }
    NetworkKind::Named(name) => {
      let ip = get_named_network_addr(name, client).await?;
      format_listen_addr(&ip, port)
    }
    NetworkKind::Other(ip) => {
      Ok(std::net::SocketAddr::new(*ip, port).to_string())
    }
  }
}

// Generate the hsts config
pub async fn gen_hsts_config(hsts: Option<Hsts>) -> Option<HstsConfig> {
  let config = if let Some(opt) = hsts {
    match opt {
      Hsts::Recommended => HstsConfig {
        max_age: 31536000,
        always: true,
        preload: false,
        include_sub_domains: false,
      },
      Hsts::Strict => HstsConfig {
        max_age: 63072000,
        always: true,
        preload: true,
        include_sub_domains: true,
      },
      Hsts::Config(hsts_config) => hsts_config,
    }
  } else {
    return None;
  };
  Some(config)
}

pub async fn gen_ssl_config(
  ssl: &ProxySsl,
  state: &SystemStateRef,
) -> IoResult<PreparedSsl> {
  match ssl {
    ProxySsl::Config(ssl_config) => Ok(PreparedSsl {
      config: ssl_config.clone(),
      files: Vec::new(),
    }),
    ProxySsl::Secret(secret) => {
      let secret = state.client.inspect_secret(secret).await?;
      let mut ssl_config =
        serde_json::from_value::<ProxySslConfig>(secret.data).map_err(
          |err| err.map_err_context(|| "Unable to deserialize ProxySslConfig"),
        )?;
      let secret_path = format!("{}/secrets/{}", state.store.dir, secret.name);
      let cert_path = format!("{secret_path}.cert");
      let mut files = vec![PreparedFile {
        path: cert_path.clone(),
        data: ssl_config.certificate.clone(),
      }];
      let key_path = format!("{secret_path}.key");
      files.push(PreparedFile {
        path: key_path.clone(),
        data: ssl_config.certificate_key.clone(),
      });
      if let Some(certificate_client) = ssl_config.certificate_client {
        let certificate_client_path = format!("{secret_path}.ca");
        files.push(PreparedFile {
          path: certificate_client_path.clone(),
          data: certificate_client,
        });
        ssl_config.certificate_client = Some(certificate_client_path);
      }
      if let Some(dh_param) = ssl_config.dhparam {
        let dh_param_path = format!("{secret_path}.pem");
        files.push(PreparedFile {
          path: dh_param_path.clone(),
          data: dh_param,
        });
        ssl_config.dhparam = Some(dh_param_path);
      }
      ssl_config.certificate = cert_path;
      ssl_config.certificate_key = key_path;
      Ok(PreparedSsl {
        config: ssl_config,
        files,
      })
    }
  }
}

pub async fn gen_upstream(
  target: &UpstreamTarget,
  state: &SystemStateRef,
) -> IoResult<PreparedUpstream> {
  let (target_name, target_namespace, target_kind) =
    parse_upstream_target(&target.key)?;
  let port = target.port;
  let local_node = get_local_node(&state.client).await?;
  let (key, content) = match target_kind.as_str() {
    "c" => {
      let cargo = state
        .client
        .inspect_cargo(&target_name, Some(&target_namespace))
        .await
        .map_err(|err| {
          err.map_err_context(|| {
            format!("Unable to inspect cargo {target_name}")
          })
        })?;
      let addresses = get_addresses(&cargo.instances, &local_node).await?;
      let key = format!("{}-{}-cargo", cargo.spec.cargo_key, port);
      let data = UPSTREAM_TEMPLATE.compile(&liquid::object!({
        "key": key,
        "port": port,
        "addresses": addresses,
      }))?;
      (key, data)
    }
    "v" => {
      let vm = state
        .client
        .inspect_vm(&target_name, Some(&target_namespace))
        .await
        .map_err(|err| {
          err.map_err_context(|| format!("Unable to inspect vm {target_name}"))
        })?;
      let addresses = get_addresses(&vm.instances, &local_node).await?;
      let key = format!("{}-{}-vm", vm.spec.vm_key, port);
      let data = UPSTREAM_TEMPLATE.compile(&liquid::object!({
        "key": key,
        "port": port,
        "addresses": addresses,
      }))?;
      (key, data)
    }
    _ => {
      return Err(IoError::invalid_data(
        "UpstreamTarget",
        &format!("Unknown Kind {}", target_kind),
      ));
    }
  };
  Ok(PreparedUpstream { key, content })
}

pub async fn gen_unix_target(unix: &UnixTarget) -> IoResult<PreparedUpstream> {
  let upstream_key = format!("unix-{}", unix.unix_path.replace('/', "-"));
  let data = UNIX_UPSTREAM_TEMPLATE.compile(&liquid::object!({
    "upstream_key": upstream_key,
    "path": unix.unix_path,
  }))?;
  Ok(PreparedUpstream {
    key: upstream_key,
    content: data,
  })
}

pub async fn gen_stream_upstream(
  target: &StreamTarget,
  state: &SystemStateRef,
) -> IoResult<PreparedUpstream> {
  match target {
    StreamTarget::Upstream(upstream) => gen_upstream(upstream, state).await,
    StreamTarget::Unix(unix) => gen_unix_target(unix).await,
    StreamTarget::Uri(_) => {
      Err(IoError::invalid_input("StreamTarget", "uri not supported"))
    }
  }
}

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use nanocld_client::{
    bollard_next::service::{
      ContainerInspectResponse, ContainerState, ContainerStateStatusEnum,
      EndpointSettings, HostConfig, NetworkSettings,
    },
    stubs::process::{Process, ProcessKind},
  };

  use super::*;

  fn process(
    name: &str,
    node: &str,
    mode: &str,
    status: ContainerStateStatusEnum,
    networks: &[(&str, &str)],
  ) -> Process {
    let networks = networks
      .iter()
      .map(|(name, address)| {
        (
          (*name).to_owned(),
          EndpointSettings {
            ip_address: Some((*address).to_owned()),
            ..Default::default()
          },
        )
      })
      .collect::<HashMap<_, _>>();
    Process {
      key: name.to_owned(),
      created_at: chrono::Utc::now().naive_utc(),
      updated_at: chrono::Utc::now().naive_utc(),
      name: name.to_owned(),
      kind: ProcessKind::Cargo,
      node_name: node.to_owned(),
      kind_key: "api.global.c".to_owned(),
      data: ContainerInspectResponse {
        host_config: Some(HostConfig {
          network_mode: Some(mode.to_owned()),
          ..Default::default()
        }),
        network_settings: Some(NetworkSettings {
          networks: Some(networks),
          ..Default::default()
        }),
        state: Some(ContainerState {
          status: Some(status),
          ..Default::default()
        }),
        ..Default::default()
      },
    }
  }

  #[ntex::test]
  async fn addresses_follow_each_process_actual_network_mode() {
    let process = process(
      "api-1",
      "node-a",
      "private-api",
      ContainerStateStatusEnum::RUNNING,
      &[("nanoclbr0", "172.18.0.10"), ("private-api", "172.30.0.10")],
    );
    assert_eq!(
      get_addresses(&[process], "node-a").await.unwrap(),
      vec!["172.30.0.10"]
    );
  }

  #[ntex::test]
  async fn addresses_exclude_rollout_init_stopped_and_remote_processes() {
    let running = ContainerStateStatusEnum::RUNNING;
    let processes = vec![
      process(
        "api-1",
        "node-a",
        "private",
        running,
        &[("private", "10.0.0.2")],
      ),
      process(
        "tmp-api-2",
        "node-a",
        "private",
        running,
        &[("private", "10.0.0.3")],
      ),
      process(
        "init-api-2",
        "node-a",
        "private",
        running,
        &[("private", "10.0.0.4")],
      ),
      process(
        "api-stopped",
        "node-a",
        "private",
        ContainerStateStatusEnum::EXITED,
        &[("private", "10.0.0.5")],
      ),
      process(
        "api-remote",
        "node-b",
        "private",
        running,
        &[("private", "10.0.0.6")],
      ),
    ];
    assert_eq!(
      get_addresses(&processes, "node-a").await.unwrap(),
      vec!["10.0.0.2"]
    );
  }

  #[test]
  fn host_network_process_uses_local_host_address() {
    let process = process(
      "vm-1",
      "node-a",
      "host",
      ContainerStateStatusEnum::RUNNING,
      &[],
    );
    assert_eq!(
      process_network_address(&process).as_deref(),
      Some("127.0.0.1")
    );
  }
}
