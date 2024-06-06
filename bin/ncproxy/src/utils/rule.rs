use nanocl_error::io::{IoResult, IoError, FromIo};

use nanocld_client::{
  NanocldClient,
  stubs::{
    process::Process,
    proxy::{
      UpstreamTarget, StreamTarget, UnixTarget, ProxySsl, ProxySslConfig,
    },
  },
};

use crate::models::{
  SystemStateRef, NginxRuleKind, UPSTREAM_TEMPLATE, UNIX_UPSTREAM_TEMPLATE,
};

/// Get public address of host
async fn get_host_addr(client: &NanocldClient) -> IoResult<String> {
  let info = client
    .info()
    .await
    .map_err(|err| err.map_err_context(|| "Unable to get host info"))?;
  Ok(info.host_gateway)
}

async fn get_namespace_addr(
  name: &str,
  client: &NanocldClient,
) -> IoResult<String> {
  let namespace = client.inspect_namespace(name).await.map_err(|err| {
    err.map_err_context(|| format!("Unable to inspect namespace {name}"))
  })?;
  let ipam = namespace.network.ipam.unwrap_or_default();
  let ipam_config = ipam.config.unwrap_or_default();
  let ipam_config = ipam_config
    .first()
    .ok_or(IoError::invalid_data("IpamConfig", "Unable to get index 0"))?;
  let ip_address = ipam_config
    .gateway
    .clone()
    .ok_or(IoError::invalid_data("IpamConfig", "Unable to get gateway"))?;
  Ok(ip_address)
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

pub async fn get_addresses(
  processes: &[Process],
  network: &str,
) -> IoResult<Vec<String>> {
  let mut addresses = vec![];
  for process in processes {
    log::debug!("get_addresses from: {}", process.name);
    if process.name.starts_with("tmp-") {
      continue;
    }
    let networks = process
      .data
      .network_settings
      .clone()
      .unwrap_or_default()
      .networks
      .unwrap_or_default();
    let network = networks.get(network);
    let Some(network) = network else {
      continue;
    };
    let Some(ip_address) = network.ip_address.clone() else {
      continue;
    };
    if ip_address.is_empty() {
      continue;
    }
    addresses.push(ip_address);
  }
  if addresses.is_empty() {
    return Err(IoError::invalid_data(
      "Process",
      &format!("No address found for {network} are processes running ?"),
    ));
  }
  Ok(addresses)
}

pub async fn get_network_addr(
  network: &str,
  port: u16,
  client: &NanocldClient,
) -> IoResult<String> {
  match network {
    "All" => Ok(format!("{port}")),
    "Public" => {
      let ip = get_host_addr(client).await?;
      Ok(format!("{ip}:{port}"))
    }
    "Internal" => Ok(format!("127.0.0.1:{port}")),
    network if network.ends_with(".nsp") => {
      let namespace = network.trim_end_matches(".nsp");
      let ip_address = get_namespace_addr(namespace, client).await?;
      Ok(format!("{ip_address}:{port}"))
    }
    _ => Err(IoError::invalid_data(
      "Network",
      &format!("invalid network {network}"),
    )),
  }
}

pub async fn gen_ssl_config(
  ssl: &ProxySsl,
  state: &SystemStateRef,
) -> IoResult<ProxySslConfig> {
  match ssl {
    ProxySsl::Config(ssl_config) => Ok(ssl_config.clone()),
    ProxySsl::Secret(secret) => {
      let secret = state.client.inspect_secret(secret).await?;
      let mut ssl_config =
        serde_json::from_value::<ProxySslConfig>(secret.data).map_err(
          |err| err.map_err_context(|| "Unable to deserialize ProxySslConfig"),
        )?;
      let secret_path = format!("{}/secrets/{}", state.store.dir, secret.name);
      let cert_path = format!("{secret_path}.cert");
      tokio::fs::write(&cert_path, ssl_config.certificate.clone()).await?;
      let key_path = format!("{secret_path}.key");
      tokio::fs::write(&key_path, ssl_config.certificate_key.clone()).await?;
      if let Some(certificate_client) = ssl_config.certificate_client {
        let certificate_client_path = format!("{secret_path}.ca");
        tokio::fs::write(&certificate_client_path, certificate_client).await?;
        ssl_config.certificate_client = Some(certificate_client_path);
      }
      if let Some(dh_param) = ssl_config.dhparam {
        let dh_param_path = format!("{secret_path}.pem");
        tokio::fs::write(&dh_param_path, dh_param).await?;
        ssl_config.dhparam = Some(dh_param_path);
      }
      ssl_config.certificate = cert_path;
      ssl_config.certificate_key = key_path;
      Ok(ssl_config)
    }
  }
}

pub async fn gen_upstream(
  target: &UpstreamTarget,
  kind: &NginxRuleKind,
  state: &SystemStateRef,
) -> IoResult<String> {
  let (target_name, target_namespace, target_kind) =
    parse_upstream_target(&target.key)?;
  let port = target.port;
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
      let addresses =
        get_addresses(&cargo.instances, &target_namespace).await?;
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
      let addresses = get_addresses(&vm.instances, &target_namespace).await?;
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
      ))
    }
  };
  state.store.write_conf_file(&key, &content, kind).await?;
  Ok(key)
}

pub async fn gen_unix_target_key(
  unix: &UnixTarget,
  kind: &NginxRuleKind,
  state: &SystemStateRef,
) -> IoResult<String> {
  let upstream_key = format!("unix-{}", unix.unix_path.replace('/', "-"));
  let data = UNIX_UPSTREAM_TEMPLATE.compile(&liquid::object!({
    "upstream_key": upstream_key,
    "path": unix.unix_path,
  }))?;
  state
    .store
    .write_conf_file(&upstream_key, &data, kind)
    .await?;
  Ok(upstream_key)
}

pub async fn gen_stream_upstream_key(
  target: &StreamTarget,
  state: &SystemStateRef,
) -> IoResult<String> {
  match target {
    StreamTarget::Upstream(upstream) => {
      gen_upstream(upstream, &NginxRuleKind::Stream, state).await
    }
    StreamTarget::Unix(unix) => {
      gen_unix_target_key(unix, &NginxRuleKind::Stream, state).await
    }
    StreamTarget::Uri(_) => {
      Err(IoError::invalid_input("StreamTarget", "uri not supported"))
    }
  }
}
