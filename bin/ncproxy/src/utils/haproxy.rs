use std::{fs, sync::Arc};

use futures::StreamExt;
use ntex::web;

use nanocl_error::io::{IoError, IoResult};

use nanocld_client::{
  NanocldClient,
  bollard_next::exec::{CreateExecOptions, StartExecOptions},
  stubs::proxy::{LocationTarget, ProxyRule, ResourceProxyRule},
};

use crate::models::{
  CONF_TEMPLATE, DomainMapEntry, HTTP_TEMPLATE, LocationTemplate,
  ProxyRuleState, STREAM_TEMPLATE, SystemStateRef,
};

/// Get the path to HAProxy admin socket
fn get_socket_path(state: &SystemStateRef) -> String {
  format!("{}/admin.sock", state.store.dir)
}

/// Get the path to domain map file
fn get_domain_map_path(state: &SystemStateRef) -> String {
  format!("{}/domain.map", state.store.dir)
}

/// Get the path to state files directory
fn get_state_dir(state: &SystemStateRef) -> String {
  format!("{}/state", state.store.dir)
}

/// Save proxy rule state to file for recovery
async fn save_rule_state(
  name: &str,
  rule_state: &ProxyRuleState,
  state: &SystemStateRef,
) -> IoResult<()> {
  let state_dir = get_state_dir(state);
  fs::create_dir_all(&state_dir)?;

  let state_file = format!("{}/{}.json", state_dir, name);
  let json = serde_json::to_string_pretty(rule_state)
    .map_err(|e| IoError::invalid_data("ProxyRuleState", &e.to_string()))?;

  fs::write(&state_file, json)?;
  log::debug!("Saved rule state to {}", state_file);
  Ok(())
}

/// Load proxy rule state from file
async fn load_rule_state(
  name: &str,
  state: &SystemStateRef,
) -> IoResult<ProxyRuleState> {
  let state_file = format!("{}/{}.json", get_state_dir(state), name);
  let json = fs::read_to_string(&state_file)?;

  serde_json::from_str(&json)
    .map_err(|e| IoError::invalid_data("ProxyRuleState", &e.to_string()))
}

/// Delete proxy rule state file
async fn delete_rule_state(name: &str, state: &SystemStateRef) -> IoResult<()> {
  let state_file = format!("{}/{}.json", get_state_dir(state), name);
  if std::path::Path::new(&state_file).exists() {
    fs::remove_file(&state_file)?;
  }
  Ok(())
}

/// Recover all backends and domain mappings from state files after restart
pub async fn recover_from_state(state: &SystemStateRef) -> IoResult<()> {
  let state_dir = get_state_dir(state);
  if !std::path::Path::new(&state_dir).exists() {
    log::debug!("No state directory found, skipping recovery");
    return Ok(());
  }

  let socket_path = get_socket_path(state);
  let domain_map_path = get_domain_map_path(state);

  log::info!("Recovering HAProxy runtime state from {}", state_dir);

  let entries = fs::read_dir(&state_dir)?;
  let mut all_domains = Vec::new();

  for entry in entries.flatten() {
    let path = entry.path();
    if path.extension().and_then(|s| s.to_str()) == Some("json") {
      if let Ok(json) = fs::read_to_string(&path) {
        if let Ok(rule_state) = serde_json::from_str::<ProxyRuleState>(&json) {
          log::info!("Recovering rule: {}", rule_state.name);

          // Recreate all backends
          for backend in &rule_state.backends {
            if let Err(e) =
              super::haproxy_socket::sync_backend(&socket_path, backend)
            {
              log::warn!("Failed to recover backend {}: {}", backend.name, e);
            } else {
              log::debug!("Recovered backend: {}", backend.name);
            }
          }

          // Collect domain mappings
          all_domains.extend(rule_state.domain_mappings.clone());
        }
      }
    }
  }

  // Sync all domain mappings to map file
  if !all_domains.is_empty() {
    if let Err(e) = super::haproxy_socket::sync_domain_map(
      &socket_path,
      &domain_map_path,
      &all_domains,
    ) {
      log::warn!("Failed to sync domain map: {}", e);
    } else {
      log::info!("Recovered {} domain mappings", all_domains.len());
    }
  }

  log::info!("HAProxy runtime state recovery complete");
  Ok(())
}

/// Rebuild HAProxy config parts:
/// - haproxy.cfg   => base (global/defaults)
/// - frontends.cfg => minimal frontends with map-based routing
/// - streams.cfg   => TCP/UDP frontends
/// - domain.map    => domain to backend routing map (updated via socket API)
async fn rebuild_config(state: &SystemStateRef) -> IoResult<()> {
  let fends_path = format!("{}/frontends.cfg", state.store.dir);
  let streams_path = format!("{}/streams.cfg", state.store.dir);
  let domain_map_path = get_domain_map_path(state);

  // Collect unique bind addresses from metadata
  use std::collections::HashSet;
  let mut http_binds: HashSet<String> = HashSet::new();
  let mut https_binds: HashSet<String> = HashSet::new();

  let routes_enabled_dir = format!("{}/routes-enabled", state.store.dir);
  if let Ok(entries) = fs::read_dir(&routes_enabled_dir) {
    for entry in entries.flatten() {
      if let Ok(content) = fs::read_to_string(entry.path()) {
        for line in content.lines() {
          let trimmed = line.trim_start();
          if trimmed.starts_with("# BIND_HTTP: ") {
            if let Some(bind) = trimmed.strip_prefix("# BIND_HTTP: ") {
              http_binds.insert(bind.to_string());
            }
          } else if trimmed.starts_with("# BIND_HTTPS: ") {
            if let Some(bind) = trimmed.strip_prefix("# BIND_HTTPS: ") {
              https_binds.insert(bind.to_string());
            }
          }
        }
      }
    }
  }

  // Default to *:80 and *:443 if no binds found
  if http_binds.is_empty() {
    http_binds.insert("*:80".to_string());
  }
  if https_binds.is_empty() {
    https_binds.insert("*:443".to_string());
  }

  // Initialize domain map file
  if !std::path::Path::new(&domain_map_path).exists() {
    fs::write(&domain_map_path, "")?;
  }

  let mut build_frontends = String::new();

  // Default 404 backend
  build_frontends.push_str("\n# Default 404 backend\nbackend bk_default_404\n  mode http\n  http-request return status 404 content-type text/html lf-string \"<html><body><h1>404 Not Found</h1></body></html>\"\n\n");

  // Collect all backends from routes-enabled files
  let mut all_backends = String::new();
  if let Ok(entries) = fs::read_dir(&routes_enabled_dir) {
    let mut files: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    files.sort();
    for path in files {
      if let Ok(content) = fs::read_to_string(&path) {
        let mut in_backend = false;
        for line in content.lines() {
          let trimmed = line.trim_start();
          if trimmed.starts_with("backend bk_") {
            in_backend = true;
            all_backends.push_str(line);
            all_backends.push('\n');
          } else if in_backend {
            all_backends.push_str(line);
            all_backends.push('\n');
            if trimmed.is_empty() {
              in_backend = false;
            }
          }
        }
      }
    }
  }

  // Add all backends to config
  build_frontends.push_str(&all_backends);

  // Create minimal HTTP frontends using map file for routing
  let mut http_bind_list: Vec<_> = http_binds.iter().collect();
  http_bind_list.sort();

  for bind in http_bind_list {
    let frontend_name = format!(
      "ft_http_{}",
      bind.replace(':', "_").replace('.', "_").replace('*', "all")
    );
    build_frontends.push_str(&format!(
      "\n# HTTP frontend for {}\nfrontend {}\n  bind {}\n  mode http\n  option forwardfor\n  capture request header Host len 128\n  # Use map file for domain -> backend routing\n  use_backend %[req.hdr(host),lower,map({},bk_default_404)]\n\n",
      bind, frontend_name, bind, domain_map_path
    ));
  }

  // Create HTTPS frontends per unique bind address with SSL certs
  let secrets_dir = format!("{}/secrets", state.store.dir);
  let mut pem_files: Vec<String> = Vec::new();
  if let Ok(entries) = fs::read_dir(&secrets_dir) {
    for entry in entries.flatten() {
      let path = entry.path();
      if let Some(ext) = path.extension().and_then(|e| e.to_str())
        && ext.eq_ignore_ascii_case("pem")
        && let Some(p) = path.to_str()
      {
        pem_files.push(p.to_string());
      }
    }
  }
  pem_files.sort();

  if !pem_files.is_empty() {
    let crt_list_path = format!("{}/crt-list.txt", secrets_dir);
    let mut list_body = String::new();
    for p in &pem_files {
      let cert_name = std::path::Path::new(p)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("");
      let cert_ca_path = format!("{}/{}.ca", secrets_dir, cert_name);

      if std::path::Path::new(&cert_ca_path).exists() {
        list_body.push_str(&format!(
          "{} [ca-file {} verify required]\n",
          p, cert_ca_path
        ));
      } else {
        list_body.push_str(p);
        list_body.push('\n');
      }
    }
    fs::write(&crt_list_path, list_body)?;

    let mut https_bind_list: Vec<_> = https_binds.iter().collect();
    https_bind_list.sort();

    for bind in https_bind_list {
      let frontend_name = format!(
        "ft_https_{}",
        bind.replace(':', "_").replace('.', "_").replace('*', "all")
      );
      build_frontends.push_str(&format!(
        "\n# HTTPS frontend for {}\nfrontend {}\n  bind {} ssl crt-list {} alpn h2,http/1.1\n  mode http\n  option forwardfor\n  capture request header Host len 128\n  # Use map file for domain -> backend routing\n  use_backend %[req.hdr(host),lower,map({},bk_default_404)]\n\n",
        bind, frontend_name, bind, crt_list_path, domain_map_path
      ));
    }
  }

  // Write frontends file
  fs::write(&fends_path, build_frontends)?; // Build and write streams/backends file
  let streams_enabled_dir = format!("{}/streams-enabled", state.store.dir);
  let mut streams_body = String::new();
  if let Ok(entries) = fs::read_dir(&streams_enabled_dir) {
    let mut files: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    files.sort();
    for path in files {
      if let Ok(content) = fs::read_to_string(&path) {
        streams_body.push('\n');
        streams_body.push_str(&content);
      }
    }
  }
  fs::write(&streams_path, streams_body)?;

  log::trace!(
    "HaproxyManager: rebuilt config parts at {} (frontends) and {} (streams)",
    fends_path,
    streams_path
  );
  Ok(())
}

/// Sync backends and domain mappings from files to HAProxy runtime
/// This runs after rebuild_config to push backends via socket API
async fn sync_to_runtime(state: &SystemStateRef) -> IoResult<()> {
  let domain_map_path = get_domain_map_path(state);

  log::debug!("Building domain map from route files");

  let routes_enabled_dir = format!("{}/routes-enabled", state.store.dir);
  let mut all_domains: Vec<DomainMapEntry> = Vec::new();

  // Parse all route files to extract domain mappings
  if let Ok(entries) = fs::read_dir(&routes_enabled_dir) {
    let mut files: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    files.sort();

    for path in files {
      if let Ok(content) = fs::read_to_string(&path) {
        let mut current_domain: Option<String> = None;

        for line in content.lines() {
          let trimmed = line.trim();

          // Extract domain from ACL
          if trimmed.starts_with("acl host_") && trimmed.contains("hdr(host)") {
            if let Some(domain_part) = trimmed.split("-i").nth(1) {
              current_domain = Some(domain_part.trim().to_string());
            }
          }

          // Parse use_backend to create domain mapping
          if trimmed.starts_with("use_backend bk_") && current_domain.is_some()
          {
            if let Some(backend_name) = trimmed.split_whitespace().nth(1) {
              all_domains.push(DomainMapEntry {
                domain: current_domain.clone().unwrap(),
                backend: backend_name.to_string(),
              });
              break; // Found mapping for this domain, move to next file
            }
          }
        }
      }
    }
  }

  // Build domain map file content
  let mut map_content = String::new();
  for mapping in &all_domains {
    map_content.push_str(&format!("{} {}\n", mapping.domain, mapping.backend));
  }

  // Write domain map file (for persistence and initial load)
  fs::write(&domain_map_path, &map_content)?;

  // Update HAProxy's runtime map via socket API (no reload needed!)
  log::debug!("Updating HAProxy runtime map via socket API");
  let socket_path = format!("{}/admin.sock", state.store.dir);
  if let Err(e) =
    super::haproxy_socket::clear_map(&socket_path, &domain_map_path)
  {
    log::warn!("Failed to clear HAProxy runtime map: {} (continuing)", e);
  }
  if let Err(e) = super::haproxy_socket::sync_domain_map(
    &socket_path,
    &domain_map_path,
    &all_domains,
  ) {
    log::warn!(
      "Failed to sync domain map to HAProxy runtime: {} (continuing)",
      e
    );
  }

  log::info!("Domain map updated with {} mappings", all_domains.len());
  Ok(())
}

pub async fn ensure_conf(state: &SystemStateRef) -> IoResult<()> {
  let state_ref = Arc::clone(state);
  let conf_path = format!("{}/haproxy.cfg", state_ref.store.dir);
  let fends_path = format!("{}/frontends.cfg", state_ref.store.dir);
  let streams_path = format!("{}/streams.cfg", state_ref.store.dir);

  // Check for global DH param file
  let dh_param_path = format!("{}/secrets/dhparam.pem", state_ref.store.dir);
  let ssl_dh_param = if std::path::Path::new(&dh_param_path).exists() {
    Some(dh_param_path)
  } else {
    None
  };

  // Read timeout configuration from environment or use defaults
  let timeout_connect = std::env::var("HAPROXY_TIMEOUT_CONNECT").ok();
  let timeout_client = std::env::var("HAPROXY_TIMEOUT_CLIENT").ok();
  let timeout_server = std::env::var("HAPROXY_TIMEOUT_SERVER").ok();

  let default_conf = CONF_TEMPLATE.compile(&liquid::object!({
    "haproxy_dir": state_ref.haproxy_dir,
    "state_dir": state_ref.store.dir,
    "ssl_dh_param": ssl_dh_param,
    "timeout_connect": timeout_connect,
    "timeout_client": timeout_client,
    "timeout_server": timeout_server,
  }))?;
  web::block(move || {
    [
      "routes-available",
      "routes-enabled",
      "streams-available",
      "streams-enabled",
      "log",
      "secrets",
    ]
    .iter()
    .map(|name| {
      fs::create_dir_all(format!("{}/{}", state_ref.store.dir, name))?;
      Ok::<_, IoError>(())
    })
    .collect::<IoResult<Vec<()>>>()?;
    // Always (re)write base conf so template updates propagate
    fs::write(&conf_path, &default_conf)?;
    if !std::path::Path::new(&fends_path).exists() {
      fs::write(&fends_path, b"\n")?;
    }
    if !std::path::Path::new(&streams_path).exists() {
      fs::write(&streams_path, b"\n")?;
    }
    Ok::<_, IoError>(())
  })
  .await?;
  log::trace!("HaproxyManager: base conf ensured");
  rebuild_config(state).await?;
  sync_to_runtime(state).await?;
  self::test(&state.client, &state.store.dir).await?;
  Ok(())
}

pub async fn test(client: &NanocldClient, state_dir: &str) -> IoResult<()> {
  log::info!("haproxy::test: starting");
  let cmd = format!(
    "haproxy -c -f {}/haproxy.cfg -f {}/frontends.cfg -f {}/streams.cfg",
    state_dir, state_dir, state_dir
  );
  exec_haproxy_cmd(&cmd, client).await?;
  log::info!("haproxy::test: done");
  Ok(())
}

async fn exec_haproxy_cmd(cmd: &str, client: &NanocldClient) -> IoResult<()> {
  let exec_options = CreateExecOptions {
    attach_stderr: Some(true),
    attach_stdout: Some(true),
    cmd: Some(cmd.split(' ').map(|e| e.into()).collect()),
    ..Default::default()
  };
  let start_res = client
    .create_exec("nproxy", &exec_options, Some("system"))
    .await?;
  let mut start_stream = client
    .start_exec(&start_res.id, &StartExecOptions::default())
    .await?;
  let mut output = String::default();
  while let Some(output_log) = start_stream.next().await {
    let Ok(output_log) = output_log else {
      break;
    };
    output += &output_log.data;
  }
  let inspect_result = client.inspect_exec(&start_res.id).await?;
  match inspect_result.exit_code {
    Some(code) => {
      if code == 0 {
        return Ok(());
      }
      Err(IoError::other("exec", &output))
    }
    None => Ok(()),
  }
}

pub async fn reload(client: &NanocldClient, state_dir: &str) -> IoResult<()> {
  log::info!("haproxy::reload: starting");

  // First validate the config
  let test_cmd = format!(
    "haproxy -c -f {}/haproxy.cfg -f {}/frontends.cfg -f {}/streams.cfg",
    state_dir, state_dir, state_dir
  );
  exec_haproxy_cmd(&test_cmd, client).await?;

  // Trigger reload via master CLI socket in worker mode
  // The master process will spawn new workers with updated config and gracefully shutdown old ones
  let reload_cmd = format!(
    "echo 'reload' | nc -U -w 1 {}/admin.sock 2>/dev/null || kill -USR2 $(cat /run/haproxy.pid 2>/dev/null) 2>/dev/null || true",
    state_dir
  );

  let exec_options = CreateExecOptions {
    attach_stderr: Some(true),
    attach_stdout: Some(true),
    cmd: Some(vec![
      "sh".to_string(),
      "-c".to_string(),
      reload_cmd.to_string(),
    ]),
    ..Default::default()
  };
  let start_res = client
    .create_exec("nproxy", &exec_options, Some("system"))
    .await?;
  let mut start_stream = client
    .start_exec(&start_res.id, &StartExecOptions::default())
    .await?;
  let mut output = String::default();
  while let Some(output_log) = start_stream.next().await {
    let Ok(output_log) = output_log else {
      break;
    };
    output += &output_log.data;
  }
  let inspect_result = client.inspect_exec(&start_res.id).await?;
  match inspect_result.exit_code {
    Some(code) => {
      if code == 0 {
        log::info!("haproxy::reload: done");
        return Ok(());
      }
      Err(IoError::other("exec", &output))
    }
    None => {
      log::info!("haproxy::reload: done");
      Ok(())
    }
  }
}

/// Rebuild config and reload HAProxy (used by event handler for batched reloads)
pub async fn rebuild_and_reload(
  client: &NanocldClient,
  state_dir: &str,
) -> IoResult<()> {
  log::info!("haproxy::rebuild_and_reload: starting");

  // Create a temporary state for rebuild
  let store = crate::models::Store::new(state_dir);
  let state = Arc::new(crate::models::SystemState {
    client: client.clone(),
    event_emitter: crate::models::EventEmitter::new(
      client,
      state_dir.to_string(),
    ),
    store,
    haproxy_dir: "/usr/local/etc/haproxy".to_string(),
  });

  // Try to update dynamically via Runtime API first (zero downtime)
  let socket_path = format!("{}/admin.sock", state_dir);
  let domain_map_path = format!("{}/domain.map", state_dir);

  // Parse current route files to extract backend/server info
  let routes_enabled_dir = format!("{}/routes-enabled", state_dir);
  let mut backends: std::collections::HashMap<
    String,
    Vec<(String, String, u16)>,
  > = std::collections::HashMap::new();
  let mut domains: Vec<DomainMapEntry> = Vec::new();

  if let Ok(entries) = fs::read_dir(&routes_enabled_dir) {
    let mut files: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    files.sort();

    for path in files {
      if let Ok(content) = fs::read_to_string(&path) {
        let mut current_backend: Option<String> = None;
        let mut current_domain: Option<String> = None;

        for line in content.lines() {
          let trimmed = line.trim();

          // Extract domain from ACL
          if trimmed.starts_with("acl host_") && trimmed.contains("hdr(host)") {
            if let Some(domain_part) = trimmed.split("-i").nth(1) {
              current_domain = Some(domain_part.trim().to_string());
            }
          }

          // Extract backend name and create domain mapping
          if trimmed.starts_with("use_backend bk_") {
            if let Some(backend_name) = trimmed.split_whitespace().nth(1) {
              if let Some(domain) = &current_domain {
                domains.push(DomainMapEntry {
                  domain: domain.clone(),
                  backend: backend_name.to_string(),
                });
              }
            }
          }

          // Extract backend definition
          if trimmed.starts_with("backend bk_") {
            current_backend =
              trimmed.split_whitespace().nth(1).map(|s| s.to_string());
          }

          // Extract server from backend
          if trimmed.starts_with("server ") && current_backend.is_some() {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            if parts.len() >= 3 {
              let server_name = parts[1].to_string();
              let addr_port = parts[2];
              if let Some((addr, port_str)) = addr_port.rsplit_once(':') {
                if let Ok(port) = port_str.parse::<u16>() {
                  backends
                    .entry(current_backend.clone().unwrap())
                    .or_insert_with(Vec::new)
                    .push((server_name, addr.to_string(), port));
                }
              }
            }
          }
        }
      }
    }
  }

  log::info!(
    "Found {} backends with servers, {} domain mappings",
    backends.len(),
    domains.len()
  );

  // Check if streams-enabled has any files - if so, we need full reload
  let streams_enabled_dir = format!("{}/streams-enabled", state_dir);
  let has_streams = fs::read_dir(&streams_enabled_dir)
    .map(|entries| entries.count() > 0)
    .unwrap_or(false);

  if has_streams {
    log::info!("Stream rules detected - performing full reload");
    // For now, always do full reload when streams are present
    // TODO: Implement stream runtime updates
    let mut runtime_update_ok = false;

    // Skip to full reload section
    if !runtime_update_ok {
      // Rebuild config from current files
      rebuild_config(&state).await?;

      // Validate config
      self::test(client, state_dir).await?;

      // Reload HAProxy
      reload(client, state_dir).await?;

      // Sync map after reload to ensure it's current
      sync_to_runtime(&state).await?;

      log::info!("haproxy::rebuild_and_reload: done");
      return Ok(());
    }
  }

  // Try to update via Runtime API (zero downtime)
  let mut runtime_update_ok = true;

  // Update server addresses via Runtime API
  for (backend_name, servers) in &backends {
    for (server_name, addr, port) in servers {
      if let Err(e) = super::haproxy_socket::set_server_addr(
        &socket_path,
        backend_name,
        server_name,
        addr,
        *port,
      ) {
        log::warn!(
          "Runtime API update failed for {}/{}: {} - will do full reload",
          backend_name,
          server_name,
          e
        );
        runtime_update_ok = false;
        break;
      } else {
        log::info!(
          "Updated {}/{} to {}:{} via Runtime API (no downtime)",
          backend_name,
          server_name,
          addr,
          port
        );
      }
    }
    if !runtime_update_ok {
      break;
    }
  }

  // Update domain map via Runtime API
  if runtime_update_ok {
    if let Err(e) =
      super::haproxy_socket::clear_map(&socket_path, &domain_map_path)
    {
      log::warn!(
        "Failed to clear domain map via Runtime API: {} - will do full reload",
        e
      );
      runtime_update_ok = false;
    } else if let Err(e) = super::haproxy_socket::sync_domain_map(
      &socket_path,
      &domain_map_path,
      &domains,
    ) {
      log::warn!(
        "Failed to sync domain map via Runtime API: {} - will do full reload",
        e
      );
      runtime_update_ok = false;
    }
  }

  if runtime_update_ok {
    log::info!(
      "haproxy::rebuild_and_reload: completed via Runtime API (zero downtime)"
    );
    return Ok(());
  }

  // Runtime API failed, do full reload
  log::warn!(
    "Runtime API update failed, performing full config rebuild and reload"
  );

  // Rebuild config from current files
  rebuild_config(&state).await?;

  // Validate config
  self::test(client, state_dir).await?;

  // Reload HAProxy
  reload(client, state_dir).await?;

  // Sync map after reload to ensure it's current
  sync_to_runtime(&state).await?;

  log::info!("haproxy::rebuild_and_reload: done");
  Ok(())
}

pub async fn add_rule(
  name: &str,
  rule: &ResourceProxyRule,
  state: &SystemStateRef,
) -> IoResult<()> {
  let mut stream_conf = String::new();
  let mut http_conf = String::new();

  // Get paths for backup
  let stream_available =
    format!("{}/streams-available/{}.conf", state.store.dir, name);
  let stream_enabled =
    format!("{}/streams-enabled/{}.conf", state.store.dir, name);
  let http_available =
    format!("{}/routes-available/{}.conf", state.store.dir, name);
  let http_enabled =
    format!("{}/routes-enabled/{}.conf", state.store.dir, name);

  // Backup existing configs
  let stream_backup = fs::read_to_string(&stream_available).ok();
  let http_backup = fs::read_to_string(&http_available).ok();

  for rule in &rule.rules {
    match rule {
      ProxyRule::Stream(stream_rule) => {
        let listen = super::rule::get_network_addr(
          &stream_rule.network,
          stream_rule.port,
          &state.client,
        )
        .await?;
        let upstream_key = match super::rule::gen_stream_upstream_key(
          &stream_rule.target,
          state,
        )
        .await
        {
          Err(err) => {
            log::warn!("{err} {:#?}", stream_rule.target);
            continue;
          }
          Ok(upstream_key) => upstream_key,
        };
        let ssl = match &stream_rule.ssl {
          Some(ssl) => match super::rule::gen_ssl_config(ssl, state).await {
            Ok(ssl) => Some(ssl),
            Err(err) => {
              log::warn!("Not ssl found for {name} {ssl:#?} {err}");
              None
            }
          },
          None => None,
        };
        if stream_rule.ssl.is_some() && ssl.is_none() {
          log::warn!("Not ssl found for {name} {ssl:#?}");
          continue;
        }
        let protocol = format!("{}", stream_rule.protocol);
        let data = STREAM_TEMPLATE.compile(&liquid::object!({
          "listen": listen,
          "key": name,
          "upstream_key": upstream_key,
          "ssl": ssl,
          "protocol": protocol,
        }))?;
        stream_conf += &data;
      }
      ProxyRule::Http(http_rule) => {
        let mut locations = vec![];
        let listen = super::rule::get_network_addr(
          &http_rule.network,
          http_rule.port.unwrap_or(80),
          &state.client,
        )
        .await?;
        let listen_https = super::rule::get_network_addr(
          &http_rule.network,
          http_rule.port.unwrap_or(443),
          &state.client,
        )
        .await?;
        let ssl = match &http_rule.ssl {
          Some(ssl) => match super::rule::gen_ssl_config(ssl, state).await {
            Ok(ssl) => Some(ssl),
            Err(err) => {
              log::warn!("Not ssl found for {name} {ssl:#?} {err}");
              None
            }
          },
          None => None,
        };
        let hsts_config =
          super::rule::gen_hsts_config(http_rule.hsts.clone()).await;
        let http3_config = http_rule.http3.as_ref().map(|h3| match h3 {
          nanocld_client::stubs::proxy::ProxyHttp3::Bool(_) => {
            liquid::object!({})
          }
          nanocld_client::stubs::proxy::ProxyHttp3::Config(cfg) => {
            liquid::object!({
              "Hq": cfg.hq,
              "MaxConcurrentStreams": cfg.max_concurrent_streams,
              "StreamBufferSize": cfg.stream_buffer_size,
              "ActiveConnectionIdLimit": cfg.active_connection_id_limit,
              "Bpf": cfg.bpf,
              "Gso": cfg.gso,
              "HostKey": cfg.host_key,
              "Retry": cfg.retry,
            })
          }
        });
        let listen_quic_port = if http_rule.http3.is_some() {
          http_rule.port.unwrap_or(443)
        } else {
          0
        };
        for location in &http_rule.locations {
          match &location.target {
            LocationTarget::Upstream(upstream) => {
              let upstream_key = match super::rule::gen_upstream(
                upstream,
                &crate::models::ProxyRuleKind::Site,
                state,
                location.version,
              )
              .await
              {
                Err(err) => {
                  log::warn!("{err} {:#?}", upstream);
                  continue;
                }
                Ok(upstream_key) => upstream_key,
              };
              let location = LocationTemplate {
                path: location.path.clone(),
                limit_req: location.limit_req.clone(),
                upstream_key: format!("http://{upstream_key}"),
                redirect: None,
                upstream_path: upstream.path.clone().unwrap_or("/".into()),
                version: location.version,
                allowed_ips: location.allowed_ips.clone(),
                headers: location.headers.clone(),
                ssl: None,
              };
              locations.push(location);
            }
            LocationTarget::Unix(unix) => {
              let upstream_key = super::rule::gen_unix_target_key(
                unix,
                &crate::models::ProxyRuleKind::Site,
                state,
                location.version,
              )
              .await?;
              let location = LocationTemplate {
                path: location.path.clone(),
                upstream_key: format!("http://{upstream_key}"),
                redirect: None,
                limit_req: location.limit_req.clone(),
                upstream_path: "/".to_owned(),
                version: location.version,
                allowed_ips: location.allowed_ips.clone(),
                headers: location.headers.clone(),
                ssl: None,
              };
              locations.push(location);
            }
            LocationTarget::Http(http) => {
              let location = LocationTemplate {
                path: location.path.clone(),
                upstream_key: http.url.clone(),
                limit_req: location.limit_req.clone(),
                version: location.version,
                upstream_path: "/".to_owned(),
                allowed_ips: location.allowed_ips.clone(),
                headers: location.headers.clone(),
                redirect: http.redirect.clone().map(|r| format!("{r}")),
                ssl: None,
              };
              locations.push(location);
            }
          }
        }
        let data = HTTP_TEMPLATE.compile(&liquid::object!({
          "key": name,
          "listen": listen,
          "listen_https": listen_https,
          "listen_quic": listen_https,
          "listen_quic_port": listen_quic_port,
          "domain": http_rule.domain,
          "locations": locations,
          "ssl": ssl,
          "hsts": hsts_config,
          "http3": http3_config,
          "limit_req_zone": http_rule.limit_req_zone,
        }))?;
        http_conf += &data;
      }
    }
  }
  if !stream_conf.is_empty() {
    state
      .store
      .write_conf_file(
        name,
        &stream_conf,
        &crate::models::ProxyRuleKind::Stream,
      )
      .await?;
  }
  if !http_conf.is_empty() {
    state
      .store
      .write_conf_file(name, &http_conf, &crate::models::ProxyRuleKind::Site)
      .await?;
  }

  // Validate the new configuration
  rebuild_config(state).await?;
  sync_to_runtime(state).await?;
  if let Err(e) = test(&state.client, &state.store.dir).await {
    // Config validation failed - rollback changes
    log::warn!("Config validation failed, rolling back changes: {}", e);

    // Restore backups or delete new files
    if let Some(backup) = stream_backup {
      let _ = fs::write(&stream_available, backup);
    } else {
      let _ = fs::remove_file(&stream_available);
      let _ = fs::remove_file(&stream_enabled);
    }

    if let Some(backup) = http_backup {
      let _ = fs::write(&http_available, backup);
    } else {
      let _ = fs::remove_file(&http_available);
      let _ = fs::remove_file(&http_enabled);
    }

    // Rebuild config with restored state
    let _ = rebuild_config(state).await;

    return Err(IoError::invalid_data(
      "ConfigValidation",
      &format!("HAProxy config validation failed: {}", e),
    ));
  }

  // Config is valid - proceed
  Ok(())
}

pub async fn del_rule(name: &str, state: &SystemStateRef) -> IoResult<()> {
  // Backup existing configs before deletion
  let stream_available =
    format!("{}/streams-available/{}.conf", state.store.dir, name);
  let http_available =
    format!("{}/routes-available/{}.conf", state.store.dir, name);

  let stream_backup = fs::read_to_string(&stream_available).ok();
  let http_backup = fs::read_to_string(&http_available).ok();

  // Delete config files
  state
    .store
    .delete_conf_file(name, &crate::models::ProxyRuleKind::Site)
    .await;
  state
    .store
    .delete_conf_file(name, &crate::models::ProxyRuleKind::Stream)
    .await;

  // Validate the new configuration
  rebuild_config(state).await?;
  sync_to_runtime(state).await?;
  if let Err(e) = test(&state.client, &state.store.dir).await {
    // Config validation failed - rollback changes
    log::warn!("Config validation failed after delete, rolling back: {}", e);

    // Restore backups
    if let Some(backup) = stream_backup {
      let _ = fs::write(&stream_available, backup);
      let _ = std::os::unix::fs::symlink(
        &stream_available,
        format!("{}/streams-enabled/{}.conf", state.store.dir, name),
      );
    }

    if let Some(backup) = http_backup {
      let _ = fs::write(&http_available, backup);
      let _ = std::os::unix::fs::symlink(
        &http_available,
        format!("{}/routes-enabled/{}.conf", state.store.dir, name),
      );
    }

    // Rebuild config with restored state
    let _ = rebuild_config(state).await;

    return Err(IoError::invalid_data(
      "ConfigValidation",
      &format!("HAProxy config validation failed after delete: {}", e),
    ));
  }

  Ok(())
}
