use std::{
  collections::{BTreeMap, BTreeSet},
  net::IpAddr,
};

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
    system::ObjPsStatusKind,
  },
};

use crate::models::{
  SystemStateRef, UNIX_UPSTREAM_TEMPLATE, UPSTREAM_TEMPLATE,
};

const LABEL_REPLICA: &str = "io.nanocl.cargo.replica";
const LABEL_CONTAINER: &str = "io.nanocl.cargo.container";
const LABEL_ROLE: &str = "io.nanocl.cargo.role";
const LABEL_ESSENTIAL: &str = "io.nanocl.cargo.essential";

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
  if let Ok(node) = std::env::var("NANOCL_NODE") {
    let trimmed_node = node.trim();
    if !trimmed_node.is_empty() {
      return Ok(trimmed_node.to_owned());
    }
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
  let Some((resource_key, kind)) = key.rsplit_once('.') else {
    return Err(IoError::invalid_data(
      "TargetKey",
      "Invalid expected <namespace>.<name>.<kind>",
    ));
  };
  if kind.is_empty() {
    return Err(IoError::invalid_data("TargetKey", "Kind cannot be empty"));
  }
  let resource_key = resource_key
    .parse::<nanocld_client::stubs::resource_key::ResourceKey>()
    .map_err(|err| IoError::invalid_data("TargetKey", &err.to_string()))?;
  Ok((
    resource_key.name().to_owned(),
    resource_key.namespace().to_owned(),
    kind.to_owned(),
  ))
}

fn process_is_running(process: &Process) -> bool {
  matches!(
    process
      .data
      .state
      .as_ref()
      .and_then(|state| state.status.as_ref()),
    Some(
      nanocld_client::bollard_next::service::ContainerStateStatusEnum::RUNNING
    )
  )
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

fn process_labels(
  process: &Process,
) -> Option<&std::collections::HashMap<String, String>> {
  process.data.config.as_ref()?.labels.as_ref()
}

fn process_has_enabled_healthcheck(process: &Process) -> bool {
  let Some(healthcheck) = process
    .data
    .config
    .as_ref()
    .and_then(|config| config.healthcheck.as_ref())
  else {
    return false;
  };
  !matches!(
    healthcheck.test.as_deref(),
    Some([mode, ..]) if mode.eq_ignore_ascii_case("NONE")
  )
}

fn essential_app_is_ready(process: &Process) -> bool {
  if !process_is_running(process) {
    return false;
  }
  if !process_has_enabled_healthcheck(process) {
    return true;
  }
  matches!(
    process
      .data
      .state
      .as_ref()
      .and_then(|state| state.health.as_ref())
      .and_then(|health| health.status.as_ref()),
    Some(nanocld_client::bollard_next::service::HealthStatusEnum::HEALTHY)
  )
}

fn process_network_mode(process: &Process) -> Option<&str> {
  process
    .data
    .host_config
    .as_ref()
    .and_then(|config| config.network_mode.as_deref())
}

struct CargoRuntimeGroup<'a> {
  owner: CargoEndpointOwner,
  sandbox: Option<&'a Process>,
  apps: BTreeMap<String, (&'a Process, bool)>,
  retained: bool,
  invalid: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CargoEndpointOwner {
  Host,
  DirectApp,
  Sandbox,
}

enum CargoServingEndpoint {
  ValidWithAddress(String),
  ValidAddressless,
  MissingOrInvalid,
}

fn cargo_endpoint_owner(
  application_count: usize,
  cargo_network_mode: Option<&str>,
) -> CargoEndpointOwner {
  if cargo_network_mode == Some("host") {
    CargoEndpointOwner::Host
  } else if application_count == 1 {
    CargoEndpointOwner::DirectApp
  } else {
    CargoEndpointOwner::Sandbox
  }
}

impl<'a> CargoRuntimeGroup<'a> {
  fn sandbox(process: &'a Process) -> Self {
    Self {
      owner: CargoEndpointOwner::Sandbox,
      sandbox: Some(process),
      apps: BTreeMap::new(),
      retained: process.name.starts_with("tmp-"),
      invalid: false,
    }
  }

  fn missing_sandbox(retained: bool) -> Self {
    Self {
      owner: CargoEndpointOwner::Sandbox,
      sandbox: None,
      apps: BTreeMap::new(),
      retained,
      invalid: false,
    }
  }

  fn direct(retained: bool) -> Self {
    Self {
      owner: CargoEndpointOwner::DirectApp,
      sandbox: None,
      apps: BTreeMap::new(),
      retained,
      invalid: false,
    }
  }

  fn host(retained: bool) -> Self {
    Self {
      owner: CargoEndpointOwner::Host,
      sandbox: None,
      apps: BTreeMap::new(),
      retained,
      invalid: false,
    }
  }

  fn add_app(&mut self, process: &'a Process) {
    let Some(labels) = process_labels(process) else {
      self.invalid = true;
      return;
    };
    let Some(container) = labels
      .get(LABEL_CONTAINER)
      .filter(|container| !container.is_empty())
    else {
      self.invalid = true;
      return;
    };
    let essential = match labels.get(LABEL_ESSENTIAL).map(String::as_str) {
      Some("true") => true,
      Some("false") => false,
      _ => {
        self.invalid = true;
        return;
      }
    };
    if self
      .apps
      .insert(container.clone(), (process, essential))
      .is_some()
    {
      self.invalid = true;
    }
  }

  fn serving_endpoint(
    &self,
    desired_essential_apps: &BTreeSet<String>,
    enforce_desired_apps: bool,
    expected_owner: Option<CargoEndpointOwner>,
    cargo_network_mode: Option<&str>,
  ) -> CargoServingEndpoint {
    let essential_apps = self
      .apps
      .values()
      .filter_map(|(process, essential)| essential.then_some(*process))
      .collect::<Vec<_>>();
    let observed_essential_names = self
      .apps
      .iter()
      .filter_map(|(name, (_, essential))| essential.then_some(name.clone()))
      .collect::<BTreeSet<_>>();
    if self.invalid
      || expected_owner.is_some_and(|owner| owner != self.owner)
      || essential_apps.is_empty()
      // A committed active attempt is compiled from the inspected current
      // revision. Exact membership protects it against incomplete observation.
      // Retained, pre-rename Updating, and restored Fail attempts belong to a
      // previous revision whose desired set is not carried by process labels.
      || (enforce_desired_apps
        && observed_essential_names != *desired_essential_apps)
      || essential_apps
        .iter()
        .any(|process| !essential_app_is_ready(process))
    {
      return CargoServingEndpoint::MissingOrInvalid;
    }

    match self.owner {
      CargoEndpointOwner::Sandbox => {
        let Some(sandbox) =
          self.sandbox.filter(|sandbox| process_is_running(sandbox))
        else {
          return CargoServingEndpoint::MissingOrInvalid;
        };
        if process_network_mode(sandbox) == Some("none")
          && (!enforce_desired_apps || cargo_network_mode == Some("none"))
        {
          CargoServingEndpoint::ValidAddressless
        } else if let Some(address) = process_network_address(sandbox) {
          CargoServingEndpoint::ValidWithAddress(address)
        } else {
          CargoServingEndpoint::MissingOrInvalid
        }
      }
      CargoEndpointOwner::DirectApp => {
        if self.apps.len() != 1 {
          return CargoServingEndpoint::MissingOrInvalid;
        }
        let Some((process, _)) = self.apps.values().next() else {
          return CargoServingEndpoint::MissingOrInvalid;
        };
        if !process_is_running(process) {
          CargoServingEndpoint::MissingOrInvalid
        } else if matches!(process_network_mode(process), Some("host" | "none"))
        {
          CargoServingEndpoint::ValidAddressless
        } else {
          process_network_address(process).map_or(
            CargoServingEndpoint::MissingOrInvalid,
            CargoServingEndpoint::ValidWithAddress,
          )
        }
      }
      CargoEndpointOwner::Host => {
        // Cargo-level host mode has no sandbox. Container-level `none` escapes
        // do not provide an endpoint, while every host app shares the node
        // address.
        let address = self.apps.values().find_map(|(process, _)| {
          (process_is_running(process)
            && process_network_mode(process) == Some("host"))
          .then(|| process_network_address(process))
          .flatten()
        });
        if let Some(address) = address {
          CargoServingEndpoint::ValidWithAddress(address)
        } else if self
          .apps
          .values()
          .all(|(process, _)| process_network_mode(process) == Some("none"))
        {
          CargoServingEndpoint::ValidAddressless
        } else {
          CargoServingEndpoint::MissingOrInvalid
        }
      }
    }
  }
}

/// Resolve one serving endpoint per local Cargo replica.
///
/// Single-application Cargoes publish their direct application address, while
/// multi-application Cargoes publish their running sandbox address. During a
/// rollout, sandbox IDs and the retained `tmp-` marker keep generations
/// independent. Cargo-level host mode preserves its existing marker-based
/// grouping and address deduplication.
fn get_cargo_addresses(
  processes: &[Process],
  local_node: &str,
  application_count: usize,
  cargo_network_mode: Option<&str>,
  desired_essential_apps: &BTreeSet<String>,
  cargo_status: &ObjPsStatusKind,
) -> IoResult<Vec<String>> {
  let expected_owner =
    cargo_endpoint_owner(application_count, cargo_network_mode);
  let mut replicas = BTreeMap::<String, Vec<&Process>>::new();
  for process in processes {
    if process.node_name != local_node || process.name.starts_with("candidate-")
    {
      continue;
    }
    let Some(replica) = process_labels(process)
      .and_then(|labels| labels.get(LABEL_REPLICA))
      .filter(|replica| !replica.is_empty())
    else {
      continue;
    };
    replicas.entry(replica.clone()).or_default().push(process);
  }

  let mut addresses = BTreeSet::new();
  for processes in replicas.into_values() {
    let mut groups = BTreeMap::<String, CargoRuntimeGroup>::new();

    // Register every observed sandbox before assigning apps. Its Docker ID is
    // the generation-safe runtime identity carried by shared-network apps.
    for process in &processes {
      if process_labels(process)
        .and_then(|labels| labels.get(LABEL_ROLE))
        .map(String::as_str)
        != Some("sandbox")
        || process.key.is_empty()
      {
        continue;
      }
      match groups.entry(process.key.clone()) {
        std::collections::btree_map::Entry::Vacant(entry) => {
          entry.insert(CargoRuntimeGroup::sandbox(process));
        }
        std::collections::btree_map::Entry::Occupied(mut entry) => {
          entry.get_mut().invalid = true;
        }
      }
    }

    let enforce_active_topology = !matches!(
      cargo_status,
      ObjPsStatusKind::Updating | ObjPsStatusKind::Fail
    );
    for process in processes {
      if process_labels(process)
        .and_then(|labels| labels.get(LABEL_ROLE))
        .map(String::as_str)
        != Some("app")
      {
        continue;
      }
      let retained = process.name.starts_with("tmp-");
      let mode = process_network_mode(process).unwrap_or("nanoclbr0");

      if let Some(sandbox_id) = mode
        .strip_prefix("container:")
        .filter(|sandbox_id| !sandbox_id.is_empty())
      {
        let group = groups
          .entry(sandbox_id.to_owned())
          .or_insert_with(|| CargoRuntimeGroup::missing_sandbox(retained));
        if group.retained != retained
          || group.owner != CargoEndpointOwner::Sandbox
        {
          group.invalid = true;
        }
        group.add_app(process);
        continue;
      }

      if !matches!(mode, "host" | "none") {
        let key = if retained {
          "direct:tmp"
        } else {
          "direct:active"
        };
        groups
          .entry(key.to_owned())
          .or_insert_with(|| CargoRuntimeGroup::direct(retained))
          .add_app(process);
        continue;
      }

      if !retained && enforce_active_topology {
        match expected_owner {
          CargoEndpointOwner::Host => groups
            .entry("host:active".to_owned())
            .or_insert_with(|| CargoRuntimeGroup::host(retained))
            .add_app(process),
          CargoEndpointOwner::DirectApp => groups
            .entry("direct:active".to_owned())
            .or_insert_with(|| CargoRuntimeGroup::direct(retained))
            .add_app(process),
          CargoEndpointOwner::Sandbox => {}
        }
        if expected_owner != CargoEndpointOwner::Sandbox {
          continue;
        }
      }

      // A container-level host/none escape cannot carry its sandbox ID. The
      // retained marker still separates the normal one-old/one-candidate
      // rollout. Ambiguous same-generation sandboxes are invalidated without
      // poisoning an independently identifiable old or candidate group.
      let matching = groups
        .iter()
        .filter_map(|(key, group)| {
          (group.sandbox.is_some() && group.retained == retained)
            .then_some(key.clone())
        })
        .collect::<Vec<_>>();
      if let [key] = matching.as_slice() {
        groups.get_mut(key).unwrap().add_app(process);
      } else if matching.is_empty() {
        let (key, owner) = if expected_owner == CargoEndpointOwner::Host {
          (
            if retained { "host:tmp" } else { "host:active" },
            CargoEndpointOwner::Host,
          )
        } else {
          (
            if retained {
              "direct:tmp"
            } else {
              "direct:active"
            },
            CargoEndpointOwner::DirectApp,
          )
        };
        let group =
          groups.entry(key.to_owned()).or_insert_with(|| match owner {
            CargoEndpointOwner::Host => CargoRuntimeGroup::host(retained),
            CargoEndpointOwner::DirectApp => {
              CargoRuntimeGroup::direct(retained)
            }
            CargoEndpointOwner::Sandbox => unreachable!(),
          });
        group.add_app(process);
      } else {
        for key in matching {
          groups.get_mut(&key).unwrap().invalid = true;
        }
      }
    }

    let has_retained_group = groups.values().any(|group| group.retained);
    let enforce_active_desired_apps = !matches!(
      cargo_status,
      ObjPsStatusKind::Updating | ObjPsStatusKind::Fail
    );
    let mut active = Vec::new();
    let mut retained = Vec::new();
    for group in groups.into_values() {
      let enforce_desired_apps = !group.retained && enforce_active_desired_apps;
      let expected_group_owner = enforce_desired_apps.then_some(expected_owner);
      let endpoint = group.serving_endpoint(
        desired_essential_apps,
        enforce_desired_apps,
        expected_group_owner,
        cargo_network_mode,
      );
      // Keep a valid addressless generation in its lifecycle class so a
      // committed active attempt cannot be mistaken for a missing attempt.
      if matches!(endpoint, CargoServingEndpoint::MissingOrInvalid) {
        continue;
      }
      if group.retained {
        retained.push(endpoint);
      } else {
        active.push(endpoint);
      }
    }

    // Once Updating or Fail has a retained group, even an apparently ready
    // active group must not replace the rollout/rollback source of truth.
    // Before the rename (or after restoration), the sole active group remains
    // the previous committed generation. In other states, exactly one active
    // attempt wins and exactly one retained attempt is its fallback. Ambiguous
    // classes are never guessed.
    let selected = if matches!(
      cargo_status,
      ObjPsStatusKind::Updating | ObjPsStatusKind::Fail
    ) {
      if has_retained_group {
        match retained.as_slice() {
          [address] => Some(address),
          _ => None,
        }
      } else {
        match active.as_slice() {
          [address] => Some(address),
          _ => None,
        }
      }
    } else {
      match active.as_slice() {
        [address] => Some(address),
        [] => match retained.as_slice() {
          [address] => Some(address),
          _ => None,
        },
        _ => None,
      }
    };
    if let Some(CargoServingEndpoint::ValidWithAddress(address)) = selected {
      addresses.insert(address.clone());
    }
  }

  if addresses.is_empty() {
    return Err(IoError::invalid_data(
      "Cargo",
      &format!(
        "No serving Cargo replica endpoint found on local node {local_node}"
      ),
    ));
  }
  Ok(addresses.into_iter().collect())
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
  let target_key = nanocld_client::stubs::resource_key::ResourceKey::new(
    &target_name,
    &target_namespace,
  )
  .map_err(|err| IoError::invalid_input("Upstream key", &err.to_string()))?;
  let (key, content) =
    match target_kind.as_str() {
      "c" => {
        let cargo = state
          .client
          .inspect_cargo(target_key.as_str())
          .await
          .map_err(|err| {
            err.map_err_context(|| {
              format!("Unable to inspect cargo {target_name}")
            })
          })?;
        let desired_essential_apps = cargo
          .spec
          .containers
          .iter()
          .filter_map(|container| {
            container.essential.then_some(container.name.clone())
          })
          .collect::<BTreeSet<_>>();
        let addresses = get_cargo_addresses(
          &cargo.instances,
          &local_node,
          cargo.spec.containers.len(),
          cargo.spec.network_mode.as_ref().map(|mode| mode.as_str()),
          &desired_essential_apps,
          &cargo.status.actual,
        )?;
        let key = format!("{}-{}-cargo", cargo.spec.cargo_key, port);
        let data = UPSTREAM_TEMPLATE.compile(&liquid::object!({
          "key": key,
          "port": port,
          "addresses": addresses,
        }))?;
        (key, data)
      }
      "v" => {
        let vm = state.client.inspect_vm(target_key.as_str()).await.map_err(
          |err| {
            err
              .map_err_context(|| format!("Unable to inspect vm {target_name}"))
          },
        )?;
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
      ContainerConfig, ContainerInspectResponse, ContainerState,
      ContainerStateStatusEnum, EndpointSettings, Health, HealthConfig,
      HealthStatusEnum, HostConfig, NetworkSettings,
    },
    stubs::process::{Process, ProcessKind},
  };

  use super::*;

  #[derive(Clone, Copy)]
  enum CargoProcessProbe {
    None,
    Disabled,
    Starting,
    Healthy,
    Unhealthy,
  }

  use CargoProcessProbe as Probe;

  struct CargoProcessOpts<'a> {
    logical_name: &'a str,
    role: &'a str,
    essential: bool,
    status: ContainerStateStatusEnum,
    probe: CargoProcessProbe,
  }

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
      kind_key: "global.api.c".to_owned(),
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

  fn cargo_process(
    name: &str,
    replica: &str,
    node: &str,
    mode: &str,
    networks: &[(&str, &str)],
    opts: CargoProcessOpts<'_>,
  ) -> Process {
    let CargoProcessOpts {
      logical_name,
      role,
      essential,
      status,
      probe,
    } = opts;
    let mut process = process(name, node, mode, status, networks);
    let (healthcheck, health) = match probe {
      Probe::None => (None, None),
      Probe::Disabled => (
        Some(HealthConfig {
          test: Some(vec!["none".to_owned(), "ignored".to_owned()]),
          ..Default::default()
        }),
        None,
      ),
      probe => {
        let status = match probe {
          Probe::Starting => HealthStatusEnum::STARTING,
          Probe::Healthy => HealthStatusEnum::HEALTHY,
          Probe::Unhealthy => HealthStatusEnum::UNHEALTHY,
          Probe::None | Probe::Disabled => unreachable!(),
        };
        (
          Some(HealthConfig {
            test: Some(vec!["CMD".to_owned(), "true".to_owned()]),
            ..Default::default()
          }),
          Some(Health {
            status: Some(status),
            ..Default::default()
          }),
        )
      }
    };
    process.data.config = Some(ContainerConfig {
      labels: Some(HashMap::from([
        (LABEL_REPLICA.to_owned(), replica.to_owned()),
        (LABEL_CONTAINER.to_owned(), logical_name.to_owned()),
        (LABEL_ROLE.to_owned(), role.to_owned()),
        (LABEL_ESSENTIAL.to_owned(), essential.to_string()),
      ])),
      healthcheck,
      ..Default::default()
    });
    process.data.state.as_mut().unwrap().health = health;
    process
  }

  fn sandbox_attempt(
    replica: &str,
    attempt: &str,
    address: &str,
    app_status: ContainerStateStatusEnum,
    probe: Probe,
    retained: bool,
  ) -> Vec<Process> {
    let mut sandbox = cargo_process(
      &format!("sandbox-{replica}-{attempt}"),
      replica,
      "node-a",
      "private",
      &[("private", address)],
      CargoProcessOpts {
        logical_name: "_sandbox",
        role: "sandbox",
        essential: true,
        status: ContainerStateStatusEnum::RUNNING,
        probe: Probe::None,
      },
    );
    let sandbox_mode = format!("container:{}", sandbox.key);
    let mut app = cargo_process(
      &format!("api-{replica}-{attempt}"),
      replica,
      "node-a",
      &sandbox_mode,
      &[],
      CargoProcessOpts {
        logical_name: "api",
        role: "app",
        essential: true,
        status: app_status,
        probe,
      },
    );
    let mut sidecar = cargo_process(
      &format!("worker-{replica}-{attempt}"),
      replica,
      "node-a",
      &sandbox_mode,
      &[],
      CargoProcessOpts {
        logical_name: "sidecar",
        role: "app",
        essential: false,
        status: ContainerStateStatusEnum::RUNNING,
        probe: Probe::None,
      },
    );
    if retained {
      sandbox.name = format!("tmp-{}", sandbox.name);
      app.name = format!("tmp-{}", app.name);
      sidecar.name = format!("tmp-{}", sidecar.name);
    }
    vec![sandbox, app, sidecar]
  }

  fn sandbox_replica(
    replica: &str,
    address: &str,
    app_status: ContainerStateStatusEnum,
    probe: Probe,
  ) -> Vec<Process> {
    sandbox_attempt(replica, "active", address, app_status, probe, false)
  }

  fn direct_attempt(
    replica: &str,
    attempt: &str,
    mode: &str,
    networks: &[(&str, &str)],
    app_status: ContainerStateStatusEnum,
    probe: Probe,
    retained: bool,
  ) -> Vec<Process> {
    let mut app = cargo_process(
      &format!("api-{replica}-{attempt}"),
      replica,
      "node-a",
      mode,
      networks,
      CargoProcessOpts {
        logical_name: "api",
        role: "app",
        essential: true,
        status: app_status,
        probe,
      },
    );
    if retained {
      app.name = format!("tmp-{}", app.name);
    }
    vec![app]
  }

  fn direct_replica(
    replica: &str,
    mode: &str,
    networks: &[(&str, &str)],
  ) -> Vec<Process> {
    direct_attempt(
      replica,
      "active",
      mode,
      networks,
      ContainerStateStatusEnum::RUNNING,
      Probe::None,
      false,
    )
  }

  fn cargo_addresses(
    processes: &[Process],
    local_node: &str,
  ) -> IoResult<Vec<String>> {
    cargo_addresses_with_topology(
      processes,
      local_node,
      2,
      Some("private"),
      ObjPsStatusKind::Start,
    )
  }

  fn cargo_addresses_with_status(
    processes: &[Process],
    local_node: &str,
    status: ObjPsStatusKind,
  ) -> IoResult<Vec<String>> {
    cargo_addresses_with_topology(
      processes,
      local_node,
      2,
      Some("private"),
      status,
    )
  }

  fn cargo_addresses_with_topology(
    processes: &[Process],
    local_node: &str,
    application_count: usize,
    cargo_network_mode: Option<&str>,
    status: ObjPsStatusKind,
  ) -> IoResult<Vec<String>> {
    get_cargo_addresses(
      processes,
      local_node,
      application_count,
      cargo_network_mode,
      &BTreeSet::from(["api".to_owned()]),
      &status,
    )
  }

  #[test]
  fn parses_canonical_upstream_targets_with_dotted_namespaces() {
    assert_eq!(
      parse_upstream_target("team.production.api.c").unwrap(),
      (
        "api".to_owned(),
        "team.production".to_owned(),
        "c".to_owned()
      )
    );
    for key in ["api", "global.api.", "global.api/name.c"] {
      assert!(
        parse_upstream_target(key).is_err(),
        "{key:?} must be invalid"
      );
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
  fn cargo_single_app_uses_its_direct_default_or_custom_network_address() {
    for (cargo_mode, process_mode, networks, expected) in [
      (
        None,
        "nanoclbr0",
        vec![("nanoclbr0", "172.18.0.10")],
        "172.18.0.10",
      ),
      (
        Some("private-api"),
        "private-api",
        vec![("nanoclbr0", "172.18.0.11"), ("private-api", "172.30.0.10")],
        "172.30.0.10",
      ),
    ] {
      let processes = direct_replica("direct", process_mode, &networks);
      assert_eq!(
        cargo_addresses_with_topology(
          &processes,
          "node-a",
          1,
          cargo_mode,
          ObjPsStatusKind::Start,
        )
        .unwrap(),
        vec![expected]
      );
    }
  }

  #[test]
  fn cargo_single_app_ignores_init_and_per_container_network_escapes() {
    let mut direct = direct_replica(
      "direct",
      "private-api",
      &[("private-api", "172.30.0.10")],
    );
    direct.push(cargo_process(
      "init-direct",
      "direct",
      "node-a",
      "private-api",
      &[("private-api", "172.30.0.99")],
      CargoProcessOpts {
        logical_name: "migrate",
        role: "init",
        essential: true,
        status: ContainerStateStatusEnum::RUNNING,
        probe: Probe::None,
      },
    ));
    assert_eq!(
      cargo_addresses_with_topology(
        &direct,
        "node-a",
        1,
        Some("private-api"),
        ObjPsStatusKind::Start,
      )
      .unwrap(),
      vec!["172.30.0.10"]
    );

    for mode in ["host", "none"] {
      let escaped = direct_replica("escaped", mode, &[]);
      assert!(
        cargo_addresses_with_topology(
          &escaped,
          "node-a",
          1,
          Some("private-api"),
          ObjPsStatusKind::Start,
        )
        .is_err(),
        "per-container {mode} must not become a Cargo endpoint"
      );
    }
  }

  #[test]
  fn cargo_uses_one_sandbox_endpoint_and_ignores_app_network_escapes() {
    let mut processes = sandbox_replica(
      "replica-a",
      "10.0.0.2",
      ContainerStateStatusEnum::RUNNING,
      Probe::None,
    );
    processes.push(cargo_process(
      "metrics-replica-a",
      "replica-a",
      "node-a",
      "host",
      &[],
      CargoProcessOpts {
        logical_name: "metrics",
        role: "app",
        essential: false,
        status: ContainerStateStatusEnum::RUNNING,
        probe: Probe::Unhealthy,
      },
    ));
    processes.push(cargo_process(
      "misnetworked-replica-a",
      "replica-a",
      "node-a",
      "private",
      &[("private", "10.0.0.99")],
      CargoProcessOpts {
        logical_name: "misnetworked",
        role: "app",
        essential: false,
        status: ContainerStateStatusEnum::RUNNING,
        probe: Probe::None,
      },
    ));
    processes.push(cargo_process(
      "debug-replica-a",
      "replica-a",
      "node-a",
      "none",
      &[],
      CargoProcessOpts {
        logical_name: "debug",
        role: "app",
        essential: false,
        status: ContainerStateStatusEnum::RUNNING,
        probe: Probe::None,
      },
    ));

    assert_eq!(
      cargo_addresses(&processes, "node-a").unwrap(),
      vec!["10.0.0.2"]
    );
  }

  #[test]
  fn cargo_serving_readiness_accepts_no_disabled_or_healthy_checks_only() {
    let mut processes = Vec::new();
    for (replica, address, probe) in [
      ("no-check", "10.0.0.2", Probe::None),
      ("disabled", "10.0.0.3", Probe::Disabled),
      ("healthy", "10.0.0.4", Probe::Healthy),
      ("starting", "10.0.0.5", Probe::Starting),
      ("unhealthy", "10.0.0.6", Probe::Unhealthy),
    ] {
      processes.extend(sandbox_replica(
        replica,
        address,
        ContainerStateStatusEnum::RUNNING,
        probe,
      ));
    }
    processes.extend(sandbox_replica(
      "stopped",
      "10.0.0.7",
      ContainerStateStatusEnum::EXITED,
      Probe::None,
    ));

    assert_eq!(
      cargo_addresses(&processes, "node-a").unwrap(),
      vec!["10.0.0.2", "10.0.0.3", "10.0.0.4"]
    );
  }

  #[test]
  fn cargo_host_replicas_publish_one_deduplicated_host_endpoint() {
    let processes = vec![
      cargo_process(
        "api-host-a",
        "host-a",
        "node-a",
        "host",
        &[],
        CargoProcessOpts {
          logical_name: "api",
          role: "app",
          essential: true,
          status: ContainerStateStatusEnum::RUNNING,
          probe: Probe::None,
        },
      ),
      cargo_process(
        "worker-host-a",
        "host-a",
        "node-a",
        "host",
        &[],
        CargoProcessOpts {
          logical_name: "worker",
          role: "app",
          essential: false,
          status: ContainerStateStatusEnum::RUNNING,
          probe: Probe::Healthy,
        },
      ),
      cargo_process(
        "api-host-b",
        "host-b",
        "node-a",
        "host",
        &[],
        CargoProcessOpts {
          logical_name: "api",
          role: "app",
          essential: true,
          status: ContainerStateStatusEnum::RUNNING,
          probe: Probe::Disabled,
        },
      ),
    ];

    assert_eq!(
      cargo_addresses_with_topology(
        &processes,
        "node-a",
        2,
        Some("host"),
        ObjPsStatusKind::Start,
      )
      .unwrap(),
      vec!["127.0.0.1"]
    );
  }

  #[test]
  fn cargo_ignores_remote_init_and_duplicates_within_one_attempt() {
    let mut processes = sandbox_replica(
      "valid",
      "10.0.0.2",
      ContainerStateStatusEnum::RUNNING,
      Probe::None,
    );
    processes.push(cargo_process(
      "init-valid",
      "valid",
      "node-a",
      "private",
      &[],
      CargoProcessOpts {
        logical_name: "migrate",
        role: "init",
        essential: true,
        status: ContainerStateStatusEnum::EXITED,
        probe: Probe::None,
      },
    ));

    let mut duplicate_app = sandbox_replica(
      "duplicate-app",
      "10.0.0.3",
      ContainerStateStatusEnum::RUNNING,
      Probe::None,
    );
    let mut second_app = duplicate_app[1].clone();
    second_app.key = "api-duplicate-app-second".to_owned();
    second_app.name = second_app.key.clone();
    duplicate_app.push(second_app);
    processes.extend(duplicate_app);

    let mut duplicate_sandbox = sandbox_replica(
      "duplicate-sandbox",
      "10.0.0.4",
      ContainerStateStatusEnum::RUNNING,
      Probe::None,
    );
    let mut second_sandbox = duplicate_sandbox[0].clone();
    second_sandbox.name = "sandbox-duplicate-sandbox-second".to_owned();
    duplicate_sandbox.push(second_sandbox);
    processes.extend(duplicate_sandbox);

    let mut remote = sandbox_replica(
      "remote",
      "10.0.0.6",
      ContainerStateStatusEnum::RUNNING,
      Probe::None,
    );
    for process in &mut remote {
      process.node_name = "node-b".to_owned();
    }
    processes.extend(remote);

    assert_eq!(
      cargo_addresses(&processes, "node-a").unwrap(),
      vec!["10.0.0.2"]
    );
  }

  #[test]
  fn cargo_falls_back_to_ready_retained_attempt_during_rollout() {
    let mut processes = sandbox_attempt(
      "rollout",
      "old",
      "10.0.0.2",
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      true,
    );
    processes.extend(sandbox_attempt(
      "rollout",
      "candidate",
      "10.0.0.3",
      ContainerStateStatusEnum::RUNNING,
      Probe::Starting,
      false,
    ));

    // Both generations have the same replica, role, and logical app labels.
    // Their distinct sandbox IDs keep the candidate from invalidating the old
    // serving group while it is still starting.
    assert_eq!(
      cargo_addresses(&processes, "node-a").unwrap(),
      vec!["10.0.0.2"]
    );
  }

  #[test]
  fn cargo_rollout_keeps_direct_and_sandbox_generations_separate() {
    let mut one_to_multi = direct_attempt(
      "rollout",
      "old-direct",
      "private",
      &[("private", "10.0.0.2")],
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      true,
    );
    one_to_multi.extend(sandbox_attempt(
      "rollout",
      "new-sandbox",
      "10.0.0.3",
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      false,
    ));
    assert_eq!(
      cargo_addresses_with_topology(
        &one_to_multi,
        "node-a",
        2,
        Some("private"),
        ObjPsStatusKind::Updating,
      )
      .unwrap(),
      vec!["10.0.0.2"]
    );
    assert_eq!(
      cargo_addresses_with_topology(
        &one_to_multi,
        "node-a",
        2,
        Some("private"),
        ObjPsStatusKind::Start,
      )
      .unwrap(),
      vec!["10.0.0.3"]
    );

    let mut multi_to_one = sandbox_attempt(
      "rollout",
      "old-sandbox",
      "10.0.0.4",
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      true,
    );
    multi_to_one.extend(direct_attempt(
      "rollout",
      "new-direct",
      "private",
      &[("private", "10.0.0.5")],
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      false,
    ));
    assert_eq!(
      cargo_addresses_with_topology(
        &multi_to_one,
        "node-a",
        1,
        Some("private"),
        ObjPsStatusKind::Updating,
      )
      .unwrap(),
      vec!["10.0.0.4"]
    );
    assert_eq!(
      cargo_addresses_with_topology(
        &multi_to_one,
        "node-a",
        1,
        Some("private"),
        ObjPsStatusKind::Start,
      )
      .unwrap(),
      vec!["10.0.0.5"]
    );
  }

  #[test]
  fn cargo_none_rollout_keeps_retained_endpoint_only_while_updating() {
    let mut processes = sandbox_attempt(
      "rollout",
      "old",
      "10.0.0.2",
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      true,
    );
    let mut active = sandbox_attempt(
      "rollout",
      "active",
      "10.0.0.3",
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      false,
    );
    active[0].data.host_config.as_mut().unwrap().network_mode =
      Some("none".to_owned());
    processes.extend(active);

    assert_eq!(
      cargo_addresses_with_topology(
        &processes,
        "node-a",
        2,
        Some("none"),
        ObjPsStatusKind::Updating,
      )
      .unwrap(),
      vec!["10.0.0.2"]
    );

    let error = cargo_addresses_with_topology(
      &processes,
      "node-a",
      2,
      Some("none"),
      ObjPsStatusKind::Start,
    )
    .unwrap_err();
    assert_eq!(error.inner.kind(), std::io::ErrorKind::InvalidData);
    assert!(
      error
        .to_string()
        .contains("No serving Cargo replica endpoint")
    );
  }

  #[test]
  fn cargo_direct_none_rollout_keeps_retained_endpoint_only_while_updating() {
    let mut processes = direct_attempt(
      "rollout",
      "old",
      "private",
      &[("private", "10.0.0.2")],
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      true,
    );
    processes.extend(direct_attempt(
      "rollout",
      "active",
      "none",
      &[],
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      false,
    ));

    assert_eq!(
      cargo_addresses_with_topology(
        &processes,
        "node-a",
        1,
        Some("private"),
        ObjPsStatusKind::Updating,
      )
      .unwrap(),
      vec!["10.0.0.2"]
    );

    let error = cargo_addresses_with_topology(
      &processes,
      "node-a",
      1,
      Some("private"),
      ObjPsStatusKind::Start,
    )
    .unwrap_err();
    assert_eq!(error.inner.kind(), std::io::ErrorKind::InvalidData);
    assert!(
      error
        .to_string()
        .contains("No serving Cargo replica endpoint")
    );
  }

  #[test]
  fn cargo_routable_direct_rollout_hands_off_after_commit() {
    let mut processes = direct_attempt(
      "rollout",
      "old",
      "private",
      &[("private", "10.0.0.2")],
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      true,
    );
    processes.extend(direct_attempt(
      "rollout",
      "active",
      "private",
      &[("private", "10.0.0.3")],
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      false,
    ));

    assert_eq!(
      cargo_addresses_with_topology(
        &processes,
        "node-a",
        1,
        Some("private"),
        ObjPsStatusKind::Updating,
      )
      .unwrap(),
      vec!["10.0.0.2"]
    );
    assert_eq!(
      cargo_addresses_with_topology(
        &processes,
        "node-a",
        1,
        Some("private"),
        ObjPsStatusKind::Start,
      )
      .unwrap(),
      vec!["10.0.0.3"]
    );
  }

  #[test]
  fn cargo_partial_candidate_cannot_replace_complete_retained_attempt() {
    let mut processes = sandbox_attempt(
      "rollout",
      "old",
      "10.0.0.2",
      ContainerStateStatusEnum::RUNNING,
      Probe::None,
      true,
    );
    let candidate = sandbox_attempt(
      "rollout",
      "candidate",
      "10.0.0.3",
      ContainerStateStatusEnum::RUNNING,
      Probe::None,
      false,
    );
    let candidate_mode = format!("container:{}", candidate[0].key);
    processes.extend(candidate.clone());
    let desired = BTreeSet::from(["api".to_owned(), "worker".to_owned()]);

    assert_eq!(
      get_cargo_addresses(
        &processes,
        "node-a",
        2,
        Some("private"),
        &desired,
        &ObjPsStatusKind::Start,
      )
      .unwrap(),
      vec!["10.0.0.2"]
    );

    processes.push(cargo_process(
      "worker-rollout-candidate",
      "rollout",
      "node-a",
      &candidate_mode,
      &[],
      CargoProcessOpts {
        logical_name: "worker",
        role: "app",
        essential: true,
        status: ContainerStateStatusEnum::RUNNING,
        probe: Probe::None,
      },
    ));
    assert_eq!(
      get_cargo_addresses(
        &processes,
        "node-a",
        2,
        Some("private"),
        &desired,
        &ObjPsStatusKind::Start,
      )
      .unwrap(),
      vec!["10.0.0.3"]
    );
  }

  #[test]
  fn cargo_never_routes_a_healthy_unpromoted_candidate() {
    let mut processes = sandbox_attempt(
      "rollout",
      "old",
      "10.0.0.2",
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      true,
    );
    let mut candidate = sandbox_attempt(
      "rollout",
      "new",
      "10.0.0.3",
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      false,
    );
    for process in &mut candidate {
      process.name = format!("candidate-{}", process.name);
    }
    processes.extend(candidate);

    assert_eq!(
      cargo_addresses(&processes, "node-a").unwrap(),
      vec!["10.0.0.2"]
    );
  }

  #[test]
  fn cargo_updating_routes_only_the_ready_retained_attempt() {
    let mut processes = sandbox_attempt(
      "rollout",
      "old",
      "10.0.0.2",
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      true,
    );
    processes.extend(sandbox_attempt(
      "rollout",
      "active",
      "10.0.0.3",
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      false,
    ));

    assert_eq!(
      cargo_addresses_with_status(
        &processes,
        "node-a",
        ObjPsStatusKind::Updating,
      )
      .unwrap(),
      vec!["10.0.0.2"]
    );
  }

  #[test]
  fn cargo_updating_before_rename_preserves_the_ready_active_attempt() {
    let processes = sandbox_attempt(
      "rollout",
      "active",
      "10.0.0.3",
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      false,
    );

    assert_eq!(
      get_cargo_addresses(
        &processes,
        "node-a",
        2,
        Some("private"),
        &BTreeSet::from(["api".to_owned(), "new-worker".to_owned()]),
        &ObjPsStatusKind::Updating,
      )
      .unwrap(),
      vec!["10.0.0.3"]
    );
  }

  #[test]
  fn cargo_start_prefers_ready_active_over_ready_retained_attempt() {
    let mut processes = sandbox_attempt(
      "rollout",
      "old",
      "10.0.0.2",
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      true,
    );
    processes.extend(sandbox_attempt(
      "rollout",
      "active",
      "10.0.0.3",
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      false,
    ));

    assert_eq!(
      cargo_addresses_with_status(
        &processes,
        "node-a",
        ObjPsStatusKind::Start,
      )
      .unwrap(),
      vec!["10.0.0.3"]
    );
  }

  #[test]
  fn cargo_fail_preserves_a_restored_previous_revision() {
    let processes = sandbox_attempt(
      "rollout",
      "restored",
      "10.0.0.2",
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      false,
    );

    assert_eq!(
      get_cargo_addresses(
        &processes,
        "node-a",
        2,
        Some("private"),
        &BTreeSet::from(["api".to_owned(), "new-worker".to_owned()]),
        &ObjPsStatusKind::Fail,
      )
      .unwrap(),
      vec!["10.0.0.2"]
    );
  }

  #[test]
  fn cargo_fail_prefers_ready_retained_over_ready_active_attempt() {
    let mut processes = sandbox_attempt(
      "rollout",
      "old",
      "10.0.0.2",
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      true,
    );
    processes.extend(sandbox_attempt(
      "rollout",
      "active",
      "10.0.0.3",
      ContainerStateStatusEnum::RUNNING,
      Probe::Healthy,
      false,
    ));

    assert_eq!(
      cargo_addresses_with_status(&processes, "node-a", ObjPsStatusKind::Fail,)
        .unwrap(),
      vec!["10.0.0.2"]
    );
  }

  #[test]
  fn cargo_scopes_essential_host_escapes_to_their_rollout_attempt() {
    let mut processes = sandbox_attempt(
      "rollout",
      "old",
      "10.0.0.2",
      ContainerStateStatusEnum::RUNNING,
      Probe::None,
      true,
    );
    processes.push(cargo_process(
      "tmp-worker-rollout-old",
      "rollout",
      "node-a",
      "host",
      &[],
      CargoProcessOpts {
        logical_name: "worker",
        role: "app",
        essential: true,
        status: ContainerStateStatusEnum::RUNNING,
        probe: Probe::Healthy,
      },
    ));
    processes.extend(sandbox_attempt(
      "rollout",
      "candidate",
      "10.0.0.3",
      ContainerStateStatusEnum::RUNNING,
      Probe::None,
      false,
    ));
    processes.push(cargo_process(
      "worker-rollout-candidate",
      "rollout",
      "node-a",
      "host",
      &[],
      CargoProcessOpts {
        logical_name: "worker",
        role: "app",
        essential: true,
        status: ContainerStateStatusEnum::RUNNING,
        probe: Probe::Unhealthy,
      },
    ));

    assert_eq!(
      get_cargo_addresses(
        &processes,
        "node-a",
        2,
        Some("private"),
        &BTreeSet::from(["api".to_owned(), "worker".to_owned()]),
        &ObjPsStatusKind::Start,
      )
      .unwrap(),
      vec!["10.0.0.2"]
    );
  }

  #[test]
  fn cargo_host_rollout_uses_the_ready_retained_group_as_fallback() {
    let processes = vec![
      cargo_process(
        "tmp-api-host",
        "host",
        "node-a",
        "host",
        &[],
        CargoProcessOpts {
          logical_name: "api",
          role: "app",
          essential: true,
          status: ContainerStateStatusEnum::RUNNING,
          probe: Probe::Healthy,
        },
      ),
      cargo_process(
        "api-host-candidate",
        "host",
        "node-a",
        "host",
        &[],
        CargoProcessOpts {
          logical_name: "api",
          role: "app",
          essential: true,
          status: ContainerStateStatusEnum::RUNNING,
          probe: Probe::Starting,
        },
      ),
    ];

    assert_eq!(
      cargo_addresses_with_topology(
        &processes,
        "node-a",
        1,
        Some("host"),
        ObjPsStatusKind::Start,
      )
      .unwrap(),
      vec!["127.0.0.1"]
    );
  }

  #[test]
  fn cargo_topology_mismatches_do_not_fall_back_to_the_wrong_owner() {
    let direct =
      direct_replica("missing-sandbox", "private", &[("private", "10.0.0.2")]);
    assert!(
      cargo_addresses_with_topology(
        &direct,
        "node-a",
        2,
        Some("private"),
        ObjPsStatusKind::Start,
      )
      .is_err()
    );

    let legacy_sandbox = sandbox_replica(
      "unexpected-sandbox",
      "10.0.0.3",
      ContainerStateStatusEnum::RUNNING,
      Probe::None,
    );
    assert!(
      cargo_addresses_with_topology(
        &legacy_sandbox,
        "node-a",
        1,
        Some("private"),
        ObjPsStatusKind::Start,
      )
      .is_err()
    );

    let mut direct_with_unexpected_sandbox =
      direct_replica("direct-owner", "private", &[("private", "10.0.0.4")]);
    direct_with_unexpected_sandbox.push(cargo_process(
      "sandbox-direct-owner-unexpected",
      "direct-owner",
      "node-a",
      "private",
      &[("private", "10.0.0.99")],
      CargoProcessOpts {
        logical_name: "_sandbox",
        role: "sandbox",
        essential: true,
        status: ContainerStateStatusEnum::RUNNING,
        probe: Probe::None,
      },
    ));
    assert_eq!(
      cargo_addresses_with_topology(
        &direct_with_unexpected_sandbox,
        "node-a",
        1,
        Some("private"),
        ObjPsStatusKind::Start,
      )
      .unwrap(),
      vec!["10.0.0.4"]
    );

    let mut duplicate_direct =
      direct_replica("duplicate-direct", "private", &[("private", "10.0.0.5")]);
    let mut duplicate = duplicate_direct[0].clone();
    duplicate.key = "api-duplicate-direct-second".to_owned();
    duplicate.name = duplicate.key.clone();
    duplicate_direct.push(duplicate);
    assert!(
      cargo_addresses_with_topology(
        &duplicate_direct,
        "node-a",
        1,
        Some("private"),
        ObjPsStatusKind::Start,
      )
      .is_err()
    );
  }

  #[test]
  fn cargo_none_preserves_owner_specific_no_address_semantics() {
    let direct = direct_replica("direct-none", "none", &[]);
    assert!(
      cargo_addresses_with_topology(
        &direct,
        "node-a",
        1,
        Some("none"),
        ObjPsStatusKind::Start,
      )
      .is_err()
    );

    let sandbox = sandbox_replica(
      "sandbox-none",
      "10.0.0.8",
      ContainerStateStatusEnum::RUNNING,
      Probe::None,
    );
    let mut sandbox = sandbox;
    sandbox[0].data.host_config.as_mut().unwrap().network_mode =
      Some("none".to_owned());
    assert!(
      cargo_addresses_with_topology(
        &sandbox,
        "node-a",
        2,
        Some("none"),
        ObjPsStatusKind::Start,
      )
      .is_err()
    );
  }

  #[test]
  fn cargo_returns_an_error_without_a_serving_replica_endpoint() {
    let processes = sandbox_replica(
      "unhealthy",
      "10.0.0.2",
      ContainerStateStatusEnum::RUNNING,
      Probe::Unhealthy,
    );

    let error = cargo_addresses(&processes, "node-a").unwrap_err();
    assert_eq!(error.inner.kind(), std::io::ErrorKind::InvalidData);
    assert!(
      error
        .to_string()
        .contains("No serving Cargo replica endpoint")
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
