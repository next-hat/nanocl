use std::{fs, process::Command, sync::Arc, time::Duration};

use ntex::rt;
use ntex::web;

use nanocl_error::io::{IoError, IoResult};

use nanocld_client::stubs::proxy::{
  LocationTarget, ProxyRule, ResourceProxyRule,
};

use crate::models::{
  CONF_TEMPLATE, HTTP_TEMPLATE, LocationTemplate, NginxRuleKind,
  STREAM_TEMPLATE, SystemStateRef,
};

const NGINX_PID_PATH: &str = "/run/nginx.pid";

pub async fn ensure_conf(state: &SystemStateRef) -> IoResult<()> {
  let state_ref = Arc::clone(state);
  let conf_path = format!("{}/nginx.conf", state_ref.store.dir);
  let default_conf = CONF_TEMPLATE.compile(&liquid::object!({
    "nginx_dir": state_ref.nginx_dir,
    "state_dir": state_ref.store.dir,
  }))?;
  web::block(move || {
    [
      "sites-available",
      "sites-enabled",
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
    Ok::<_, IoError>(())
  })
  .await?;
  log::trace!(
    "NginxManager: writing default conf to {conf_path}:\n{default_conf}"
  );
  std::fs::write(conf_path, default_conf)?;
  self::test(&state.store.dir).await?;
  Ok(())
}

fn gen_conf_path(state_dir: &str) -> String {
  format!("{state_dir}/nginx.conf")
}

fn is_nginx_running() -> bool {
  let Ok(pid) = std::fs::read_to_string(NGINX_PID_PATH) else {
    return false;
  };
  Command::new("kill")
    .arg("-0")
    .arg(pid.trim())
    .status()
    .is_ok_and(|status| status.success())
}

fn exec_nginx_cmd(args: &[&str], conf_path: &str) -> IoResult<()> {
  let output = Command::new("nginx")
    .args(args)
    .arg("-c")
    .arg(conf_path)
    .output()
    .map_err(|err| {
      IoError::other("exec", &format!("unable to run nginx: {err}"))
    })?;
  if output.status.success() {
    return Ok(());
  }
  let mut message = String::from_utf8_lossy(&output.stdout).into_owned();
  message.push_str(String::from_utf8_lossy(&output.stderr).as_ref());
  Err(IoError::other("exec", &message))
}

pub async fn ensure_started(state_dir: &str) -> IoResult<()> {
  let conf_path = gen_conf_path(state_dir);
  web::block(move || {
    if is_nginx_running() {
      return Ok(());
    }
    let _ = std::fs::remove_file(NGINX_PID_PATH);
    exec_nginx_cmd(&[], &conf_path)
  })
  .await?;
  Ok(())
}

pub fn spawn_monitor(state_dir: &str) {
  let state_dir = state_dir.to_owned();
  rt::spawn(async move {
    loop {
      if !is_nginx_running() {
        log::warn!("nginx::monitor: nginx is not running, restarting");
        if let Err(err) = ensure_started(&state_dir).await {
          log::warn!("nginx::monitor: {err}");
        }
      }
      ntex::time::sleep(Duration::from_secs(2)).await;
    }
  });
}

pub async fn test(state_dir: &str) -> IoResult<()> {
  log::info!("nginx::test: starting");
  let conf_path = gen_conf_path(state_dir);
  web::block(move || exec_nginx_cmd(&["-t"], &conf_path)).await?;
  log::info!("nginx::test: done");
  Ok(())
}

pub async fn reload(state_dir: &str) -> IoResult<()> {
  log::info!("nginx::reload: starting");
  ensure_started(state_dir).await?;
  let conf_path = gen_conf_path(state_dir);
  web::block(move || exec_nginx_cmd(&["-s", "reload"], &conf_path)).await?;
  log::info!("nginx::reload: done");
  Ok(())
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
            Err(err) => {
              log::warn!("Not ssl found for {name} {ssl:#?} {err}");
              None
            }
            Ok(ssl) => Some(ssl),
          },
          None => None,
        };
        if stream_rule.ssl.is_some() && ssl.is_none() {
          log::warn!("Not ssl found for {name} {ssl:#?}");
          continue;
        }
        let data = STREAM_TEMPLATE.compile(&liquid::object!({
          "listen": listen,
          "key": name,
          "upstream_key": upstream_key,
          "ssl": ssl,
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
            Err(err) => {
              log::warn!("Not ssl found for {name} {ssl:#?} {err}");
              None
            }
            Ok(ssl) => Some(ssl),
          },
          None => None,
        };
        let hsts_config =
          super::rule::gen_hsts_config(http_rule.hsts.clone()).await;
        for location in &http_rule.locations {
          match &location.target {
            LocationTarget::Upstream(upstream) => {
              let upstream_key = match super::rule::gen_upstream(
                upstream,
                &NginxRuleKind::Site,
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
              let ssl = match &upstream.ssl {
                Some(ssl) => {
                  match super::rule::gen_ssl_config(ssl, state).await {
                    Err(err) => {
                      log::warn!("Not ssl found for {name} {ssl:#?} {err}");
                      None
                    }
                    Ok(ssl) => Some(ssl),
                  }
                }
                None => None,
              };
              let location = LocationTemplate {
                path: location.path.clone(),
                limit_req: location.limit_req.clone(),
                upstream_key: if ssl.is_some() {
                  format!("https://{upstream_key}")
                } else {
                  format!("http://{upstream_key}")
                },
                redirect: None,
                upstream_path: upstream.path.clone().unwrap_or("/".to_owned()),
                version: location.version,
                allowed_ips: location.allowed_ips.clone(),
                headers: location.headers.clone(),
                ssl,
              };
              locations.push(location);
            }
            LocationTarget::Unix(unix) => {
              let upstream_key = super::rule::gen_unix_target_key(
                unix,
                &NginxRuleKind::Site,
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
          "limit_req_zone": http_rule.limit_req_zone,
          "listen": listen,
          "listen_https": listen_https,
          "domain": http_rule.domain,
          "locations": locations,
          "ssl": ssl,
          "hsts": hsts_config,
          "http3": http_rule.http3,
          "hide_upstream": http_rule.ssl.is_some() && ssl.is_none(),
        }))?;
        http_conf += &data;
      }
    }
  }
  if !stream_conf.is_empty() {
    state
      .store
      .write_conf_file(name, &stream_conf, &NginxRuleKind::Stream)
      .await?;
  }
  if !http_conf.is_empty() {
    state
      .store
      .write_conf_file(name, &http_conf, &NginxRuleKind::Site)
      .await?;
  }
  if let Err(err) = self::test(&state.store.dir).await {
    let _ = del_rule(name, state).await;
    return Err(err);
  }
  Ok(())
}

pub async fn del_rule(name: &str, state: &SystemStateRef) {
  let _ = state
    .store
    .delete_conf_file(name, &NginxRuleKind::Site)
    .await;
  let _ = state
    .store
    .delete_conf_file(name, &NginxRuleKind::Stream)
    .await;
}
