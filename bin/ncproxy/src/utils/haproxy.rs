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
  CONF_TEMPLATE, HTTP_TEMPLATE, LocationTemplate, STREAM_TEMPLATE,
  SystemStateRef,
};

/// Rebuild HAProxy config parts:
/// - haproxy.cfg   => base (global/defaults)
/// - frontends.cfg => shared HTTP/HTTPS frontends + vhost routing + default 404 backend
/// - streams.cfg   => TCP/UDP frontends and HTTP backends
async fn rebuild_config(state: &SystemStateRef) -> IoResult<()> {
  let fends_path = format!("{}/frontends.cfg", state.store.dir);
  let streams_path = format!("{}/streams.cfg", state.store.dir);

  // Helper to append only route lines valid inside a frontend
  let append_route_snippets = |dst: &mut String| {
    let routes_enabled_dir = format!("{}/routes-enabled", state.store.dir);
    if let Ok(entries) = fs::read_dir(&routes_enabled_dir) {
      // Deterministic order
      let mut files: Vec<_> = entries.flatten().map(|e| e.path()).collect();
      files.sort();
      for path in files {
        if let Ok(content) = fs::read_to_string(&path) {
          dst.push_str("  \n");
          for line in content.lines() {
            let l = line.trim_start();
            let allowed = l.is_empty()
              || l.starts_with('#')
              || l.starts_with("acl ")
              || l.starts_with("use_backend ")
              || l.starts_with("http-response ");
            if allowed {
              dst.push_str("  ");
              dst.push_str(line);
              dst.push('\n');
            }
          }
        }
      }
    }
  };

  // Build frontends content
  let mut build_frontends = String::new();
  // Shared HTTP frontend on port 80 with virtual host routing
  build_frontends.push_str(
    "\nfrontend http_80\n  bind *:80\n  mode http\n  option forwardfor\n  capture request header Host len 128\n",
  );
  append_route_snippets(&mut build_frontends);
  build_frontends.push_str("  default_backend bk_default_404\n\n");

  // Shared HTTPS frontend on port 443 with SNI-based cert selection (crt-list of *.pem)
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
      list_body.push_str(p);
      list_body.push('\n');
    }
    fs::write(&crt_list_path, list_body)?;

    build_frontends.push_str(&format!(
      "frontend https_443\n  bind *:443 ssl crt-list {} alpn h2,http/1.1\n  mode http\n  option forwardfor\n  capture request header Host len 128\n",
      crt_list_path
    ));
    append_route_snippets(&mut build_frontends);
    build_frontends.push_str("  default_backend bk_default_404\n\n");
  } else {
    log::warn!(
      "haproxy::rebuild_config: no PEM files found in {}, skipping https_443 frontend",
      secrets_dir
    );
  }
  // Default 404 backend
  build_frontends.push_str("\nbackend bk_default_404\n  mode http\n  http-request return status 404 content-type text/html lf-string \"<html><body><h1>404 Not Found</h1></body></html>\"\n");

  // Write frontends file
  fs::write(&fends_path, build_frontends)?;

  // Build and write streams/backends file
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

pub async fn ensure_conf(state: &SystemStateRef) -> IoResult<()> {
  let state_ref = Arc::clone(state);
  let conf_path = format!("{}/haproxy.cfg", state_ref.store.dir);
  let fends_path = format!("{}/frontends.cfg", state_ref.store.dir);
  let streams_path = format!("{}/streams.cfg", state_ref.store.dir);
  let default_conf = CONF_TEMPLATE.compile(&liquid::object!({
    "haproxy_dir": state_ref.haproxy_dir,
    "state_dir": state_ref.store.dir,
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

  // Then do graceful reload with new config
  let reload_cmd = format!(
    "haproxy -f {}/haproxy.cfg -f {}/frontends.cfg -f {}/streams.cfg -p /run/haproxy.pid -sf $(cat /run/haproxy.pid 2>/dev/null || echo)",
    state_dir, state_dir, state_dir
  );
  let exec_options = CreateExecOptions {
    attach_stderr: Some(true),
    attach_stdout: Some(true),
    cmd: Some(vec!["sh".to_string(), "-c".to_string(), reload_cmd]),
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

pub async fn add_rule(
  name: &str,
  rule: &ResourceProxyRule,
  state: &SystemStateRef,
) -> IoResult<()> {
  let mut stream_conf = String::new();
  let mut http_conf = String::new();
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
  rebuild_config(state).await?;
  if let Err(err) = self::test(&state.client, &state.store.dir).await {
    let _ = del_rule(name, state).await;
    return Err(err);
  }
  Ok(())
}

pub async fn del_rule(name: &str, state: &SystemStateRef) {
  let _ = state
    .store
    .delete_conf_file(name, &crate::models::ProxyRuleKind::Site)
    .await;
  let _ = state
    .store
    .delete_conf_file(name, &crate::models::ProxyRuleKind::Stream)
    .await;
  let _ = rebuild_config(state).await;
}
