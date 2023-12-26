use std::{fs, sync::Arc};

use ntex::web;
use futures::StreamExt;

use nanocl_error::io::{IoError, IoResult};

use nanocld_client::{
  stubs::proxy::{ResourceProxyRule, ProxyRule, LocationTarget},
  bollard_next::exec::{CreateExecOptions, StartExecOptions},
  NanocldClient,
};

use crate::models::{
  SystemStateRef, NginxRuleKind, LocationTemplate, STREAM_TEMPLATE,
  HTTP_TEMPLATE, CONF_TEMPLATE,
};

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
    .into_iter()
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
  self::test(&state.client).await?;
  Ok(())
}

pub async fn test(client: &NanocldClient) -> IoResult<()> {
  log::info!("nginx::test: starting");
  exec_nginx_cmd("nginx -t", client).await?;
  log::info!("nginx::test: done");
  Ok(())
}

async fn exec_nginx_cmd(cmd: &str, client: &NanocldClient) -> IoResult<()> {
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

pub async fn reload(client: &NanocldClient) -> IoResult<()> {
  log::info!("nginx::reload: starting");
  exec_nginx_cmd("nginx -s reload", client).await?;
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
        let upstream_key =
          super::rule::gen_stream_upstream_key(&stream_rule.target, state)
            .await?;
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
          continue;
        }
        let data = STREAM_TEMPLATE.compile(&liquid::object!({
          "listen": listen,
          "upstream_key": upstream_key,
          "ssl": ssl,
        }))?;
        stream_conf += &data;
      }
      ProxyRule::Http(http_rule) => {
        let mut locations = vec![];
        let listen =
          super::rule::get_network_addr(&http_rule.network, 80, &state.client)
            .await?;
        let listen_https =
          super::rule::get_network_addr(&http_rule.network, 443, &state.client)
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
        if http_rule.ssl.is_some() && ssl.is_none() {
          continue;
        }
        for location in &http_rule.locations {
          match &location.target {
            LocationTarget::Upstream(upstream) => {
              let upstream_key = super::rule::gen_upstream(
                upstream,
                &NginxRuleKind::Site,
                state,
              )
              .await?;
              let location = LocationTemplate {
                path: location.path.clone(),
                upstream_key: format!("http://{upstream_key}"),
                redirect: None,
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
              };
              locations.push(location);
            }
            LocationTarget::Http(http) => {
              let location = LocationTemplate {
                path: location.path.clone(),
                upstream_key: http.url.clone(),
                redirect: http.redirect.clone().map(|r| format!("{r}")),
              };
              locations.push(location);
            }
          }
        }
        let data = HTTP_TEMPLATE.compile(&liquid::object!({
          "listen": listen,
          "listen_https": listen_https,
          "domain": http_rule.domain,
          "locations": locations,
          "ssl": ssl,
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
  if let Err(err) = self::test(&state.client).await {
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
