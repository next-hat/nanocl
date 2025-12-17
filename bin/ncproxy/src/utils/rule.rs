use nanocl_error::io::{FromIo, IoError, IoResult};

use nanocld_client::{
  NanocldClient,
  stubs::{
    generic::NetworkKind,
    process::Process,
    proxy::{
      Hsts, HstsConfig, ProxySsl, ProxySslConfig, StreamTarget, UnixTarget,
      UpstreamTarget,
    },
  },
};

use crate::models::{
  ProxyRuleKind, SystemStateRef, UNIX_UPSTREAM_TEMPLATE, UPSTREAM_TEMPLATE,
};

/// Get public address of host
async fn get_host_addr(client: &NanocldClient) -> IoResult<String> {
  let info = client
    .info()
    .await
    .map_err(|err| err.map_err_context(|| "Unable to get host info"))?;
  Ok(info.host_gateway)
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
  network: &NetworkKind,
  port: u16,
  client: &NanocldClient,
) -> IoResult<String> {
  match network {
    NetworkKind::All => Ok(format!("*:{port}")),
    NetworkKind::Public => {
      let ip = get_host_addr(client).await?;
      Ok(format!("{ip}:{port}"))
    }
    NetworkKind::Local => Ok(format!("127.0.0.1:{port}")),
    NetworkKind::Internal => {
      let ip = get_bridge_addr(client).await?;
      Ok(format!("{ip}:{port}"))
    }
    NetworkKind::Other(ip) => Ok(format!("{ip}:{port}")),
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

      // HAProxy requires a combined PEM file with certificate + key
      let combined_pem_path = format!("{secret_path}.pem");
      let combined_pem = format!(
        "{}\n{}",
        ssl_config.certificate.trim(),
        ssl_config.certificate_key.trim()
      );
      tokio::fs::write(&combined_pem_path, combined_pem).await?;

      // Also write separate cert and key files for backend SSL
      let cert_path = format!("{secret_path}.cert");
      tokio::fs::write(&cert_path, ssl_config.certificate.clone()).await?;
      let key_path = format!("{secret_path}.key");
      tokio::fs::write(&key_path, ssl_config.certificate_key.clone()).await?;

      if let Some(certificate_client) = ssl_config.certificate_client {
        let certificate_client_path = format!("{secret_path}.ca");
        tokio::fs::write(&certificate_client_path, &certificate_client).await?;
        ssl_config.certificate_client = Some(certificate_client_path);

        // Note: Per-certificate CA files are used in crt-list with [ca-file ... verify required]
        // This allows different domains to have different client certificate requirements
      }
      if let Some(dh_param) = ssl_config.dhparam {
        let dh_param_path = format!("{secret_path}.dhparam");
        tokio::fs::write(&dh_param_path, &dh_param).await?;
        ssl_config.dhparam = Some(dh_param_path);

        // Also write global dhparam.pem for global config
        let global_dh_path = format!("{}/secrets/dhparam.pem", state.store.dir);
        tokio::fs::write(&global_dh_path, dh_param).await?;
      } // Set the combined PEM path for frontend binds
      ssl_config.certificate = combined_pem_path;
      // Keep separate cert and key paths for backend connections
      ssl_config.certificate_key = key_path;
      Ok(ssl_config)
    }
  }
}

pub async fn gen_upstream(
  target: &UpstreamTarget,
  kind: &ProxyRuleKind,
  state: &SystemStateRef,
  version: Option<f64>,
) -> IoResult<String> {
  let (target_name, target_namespace, target_kind) =
    parse_upstream_target(&target.key)?;
  let port = target.port;

  // Process SSL configuration for end-to-end TLS
  let ssl_config = if let Some(ssl) = &target.ssl {
    match gen_ssl_config(ssl, state).await {
      Ok(ssl) => Some(ssl),
      Err(err) => {
        log::warn!("SSL config error for target {}: {}", target.key, err);
        None
      }
    }
  } else {
    None
  };

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
      let addresses = get_addresses(&cargo.instances, "nanoclbr0").await?;
      // Include version in key to create separate backends for different HTTP versions
      let version_suffix = version
        .map(|v| format!("-v{}", v.to_string().replace('.', "_")))
        .unwrap_or_default();
      let key =
        format!("{}-{}-cargo{}", cargo.spec.cargo_key, port, version_suffix);
      let mode = match kind {
        ProxyRuleKind::Site => "http",
        ProxyRuleKind::Stream => "tcp",
      };
      let data = UPSTREAM_TEMPLATE.compile(&liquid::object!({
        "key": key,
        "port": port,
        "addresses": addresses,
        "mode": mode,
        "ssl": ssl_config,
        "version": version,
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
      let addresses = get_addresses(&vm.instances, "nanoclbr0").await?;
      // Include version in key to create separate backends for different HTTP versions
      let version_suffix = version
        .map(|v| format!("-v{}", v.to_string().replace('.', "_")))
        .unwrap_or_default();
      let key = format!("{}-{}-vm{}", vm.spec.vm_key, port, version_suffix);
      let mode = match kind {
        ProxyRuleKind::Site => "http",
        ProxyRuleKind::Stream => "tcp",
      };
      let data = UPSTREAM_TEMPLATE.compile(&liquid::object!({
        "key": key,
        "port": port,
        "addresses": addresses,
        "mode": mode,
        "ssl": ssl_config,
        "version": version,
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
  // Write backend to the appropriate location:
  // - HTTP backends (Site) go to routes-enabled (with their frontends)
  // - TCP/UDP backends (Stream) go to streams-enabled
  state.store.write_conf_file(&key, &content, kind).await?;
  Ok(key)
}

pub async fn gen_unix_target_key(
  unix: &UnixTarget,
  kind: &ProxyRuleKind,
  state: &SystemStateRef,
  version: Option<f64>,
) -> IoResult<String> {
  let version_suffix = version
    .map(|v| format!("-v{}", v.to_string().replace('.', "_")))
    .unwrap_or_default();
  let upstream_key = format!(
    "unix-{}{}",
    unix.unix_path.replace('/', "-"),
    version_suffix
  );
  let mode = match kind {
    ProxyRuleKind::Site => "http",
    ProxyRuleKind::Stream => "tcp",
  };
  let data = UNIX_UPSTREAM_TEMPLATE.compile(&liquid::object!({
    "upstream_key": upstream_key,
    "path": unix.unix_path,
    "mode": mode,
    "version": version,
  }))?;
  // Write backend to the appropriate location based on kind
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
      gen_upstream(upstream, &ProxyRuleKind::Stream, state, None).await
    }
    StreamTarget::Unix(unix) => {
      gen_unix_target_key(unix, &ProxyRuleKind::Stream, state, None).await
    }
    StreamTarget::Uri(_) => {
      Err(IoError::invalid_input("StreamTarget", "uri not supported"))
    }
  }
}
