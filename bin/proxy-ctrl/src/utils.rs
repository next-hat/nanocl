use nanocld_client::{
  NanocldClient,
  stubs::{
    resource::{Resource, ResourceQuery},
    cargo::{CargoInspect, CargoExecConfig},
  },
};

use crate::{
  error::ErrorHint,
  models::{
    ProxyRuleHttp, CargoTarget, ProxyHttpLocation, ProxyRuleStream,
    LocationTarget,
  },
};
use crate::nginx::{Nginx, NginxConfKind};
use crate::models::{ResourceProxyRule, ProxyRule};

/// Serialize a ProxyRule
pub(crate) fn serialize_proxy_rule(
  resource: &Resource,
) -> Result<ResourceProxyRule, ErrorHint> {
  let proxy_rule =
    serde_json::from_value::<ResourceProxyRule>(resource.config.to_owned())
      .map_err(|err| {
        ErrorHint::warning(format!(
          "Unable to parse proxy rule {name}: {err}",
          name = resource.name,
        ))
      })?;
  Ok(proxy_rule)
}

fn get_listen_addr(name: &str, network: &str) -> Result<String, ErrorHint> {
  match network {
    "Public" => Ok("0.0.0.0".into()),
    "Internal" => Ok("127.0.0.1".into()),
    _ => Err(ErrorHint::warning(format!(
      "Unsupported network {network} for resource {name}"
    ))),
  }
}

fn create_cargo_upstream(
  kind: &NginxConfKind,
  port: u16,
  cargo: &CargoInspect,
  nginx: &Nginx,
) -> Result<String, ErrorHint> {
  let ip_addresses = cargo
    .instances
    .iter()
    .map(|node_container| {
      let container = node_container.container.clone();
      let networks = container
        .network_settings
        .clone()
        .unwrap_or_default()
        .networks
        .unwrap_or_default();
      let network =
        networks
          .get(&cargo.namespace_name)
          .ok_or(ErrorHint::Warning(format!(
      "Unable to find network for container {} for cargo {} in namespace {}",
      container.id.clone().unwrap_or_default(),
      cargo.key,
      cargo.namespace_name,
    )))?;
      let ip_address =
        network
          .ip_address
          .clone()
          .ok_or(ErrorHint::Warning(format!(
      "Unable to find ip address for container {} for cargo {} in namespace {}",
      container.id.unwrap_or_default(),
      cargo.key,
      cargo.namespace_name,
    )))?;
      Ok::<_, ErrorHint>(ip_address)
    })
    .collect::<Result<Vec<String>, ErrorHint>>()?;
  let upstream_key = format!("{}-{}", cargo.key, port);
  let upstream = format!(
    "
upstream {upstream_key} {{
  hash $remote_addr consistent;
  {}
}}
",
    ip_addresses
      .iter()
      .map(|ip_address| format!("  server {ip_address}:{port};"))
      .collect::<Vec<String>>()
      .join("\n")
  );
  nginx.write_conf_file(&upstream_key, &upstream, kind)?;
  Ok(upstream_key)
}

async fn gen_cargo_upstream(
  kind: &NginxConfKind,
  target: &CargoTarget,
  client: &NanocldClient,
  nginx: &Nginx,
) -> Result<String, ErrorHint> {
  let port = target.port;
  let (cargo_name, namespace) = extract_target_cargo(&target.key)?;
  let cargo = client
    .inspect_cargo(&cargo_name, Some(namespace.clone()))
    .await
    .map_err(|err| {
      ErrorHint::warning(format!(
        "Unable to inspect cargo {cargo_name} in namespace {namespace}: {err}",
      ))
    })?;
  create_cargo_upstream(kind, port, &cargo, nginx)
}

fn extract_target_cargo(key: &str) -> Result<(String, String), ErrorHint> {
  let info = key.split('.').collect::<Vec<&str>>();
  if info.len() != 2 {
    return Err(ErrorHint::warning(format!(
      "Invalid cargo key expect cargo_name@namespace got: {key}"
    )));
  }
  let namespace = info[1].to_owned();
  let name = info[0].to_owned();
  Ok((name, namespace))
}

async fn gen_locations(
  location_rules: &Vec<ProxyHttpLocation>,
  client: &NanocldClient,
  nginx: &Nginx,
) -> Result<Vec<String>, ErrorHint> {
  let mut locations = Vec::new();
  for rule in location_rules {
    let path = &rule.path;

    match &rule.target {
      LocationTarget::Cargo(cargo_target) => {
        let upstream_key =
          gen_cargo_upstream(&NginxConfKind::Site, cargo_target, client, nginx)
            .await?;
        let location = format!(
          "
  location {path} {{
    proxy_pass http://{upstream_key};
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-Scheme $scheme;
    proxy_set_header X-Forwarded-Proto  $scheme;
    proxy_set_header X-Forwarded-For    $proxy_add_x_forwarded_for;
    proxy_set_header X-Real-IP          $remote_addr;
  }}"
        );
        locations.push(location);
      }
      LocationTarget::Http(http_target) => {
        let url = http_target.url.clone();
        let location = match &http_target.redirect {
          Some(redirect) => {
            format!(
              "
  location {path} {{
    return {redirect} {url};
  }}
"
            )
          }
          None => {
            format!(
              "
  location {path} {{
    proxy_pass {url};
  }}
"
            )
          }
        };
        locations.push(location);
      }
    }
  }
  Ok(locations)
}

async fn gen_http_server_block(
  name: &str,
  rule: &ProxyRuleHttp,
  client: &NanocldClient,
  nginx: &Nginx,
) -> Result<String, ErrorHint> {
  let listen_addr = get_listen_addr(name, &rule.network)?;
  let locations = gen_locations(&rule.locations, client, nginx)
    .await?
    .join("\n");
  let http_host = match &rule.domain {
    Some(domain) => format!("  server_name {domain};"),
    None => String::default(),
  };

  let ssl = if let Some(ssl) = &rule.ssl {
    let certificate = &ssl.certificate;
    let certificate_key = &ssl.certificate_key;
    let ssl_dh_param = match &ssl.dh_param {
      Some(ssl_dh_param) => {
        format!("\n  ssl_dhparam          {ssl_dh_param};\n")
      }
      None => String::default(),
    };
    format!(
      "
  listen {listen_addr}:443 http2 ssl;

  if ($scheme != https) {{
    return 301 https://$host$request_uri;
  }}

  ssl_certificate      {certificate};
  ssl_certificate_key  {certificate_key};{ssl_dh_param}
    "
    )
  } else {
    String::default()
  };

  let includes = match &rule.includes {
    Some(includes) => includes
      .iter()
      .map(|include| format!("  include {include};"))
      .collect::<Vec<String>>()
      .join("\n"),
    None => String::default(),
  };

  let conf = format!(
    "
server {{
  listen {listen_addr}:80;
{http_host}{ssl}{includes}
{locations}
}}\n",
  );
  Ok(conf)
}

async fn gen_stream_server_block(
  resource_name: &str,
  rule: &ProxyRuleStream,
  client: &NanocldClient,
  nginx: &Nginx,
) -> Result<String, ErrorHint> {
  let port = rule.port;
  let listen_addr = get_listen_addr(resource_name, &rule.network)?;

  let upstream_key =
    gen_cargo_upstream(&NginxConfKind::Stream, &rule.target, client, nginx)
      .await?;

  let ssl = if let Some(ssl) = &rule.ssl {
    let certificate = &ssl.certificate;
    let certificate_key = &ssl.certificate_key;
    let ssl_dh_param = match &ssl.dh_param {
      Some(ssl_dh_param) => {
        format!("\n  ssl_dhparam          {ssl_dh_param};\n")
      }
      None => String::default(),
    };
    format!(
      "
    listen {listen_addr}:443 ssl;
    ssl_certificate      {certificate};
    ssl_certificate_key  {certificate_key};{ssl_dh_param}
          "
    )
  } else {
    String::default()
  };
  let conf = format!(
    "
server {{
  listen {listen_addr}:{port};
  proxy_pass {upstream_key};
{ssl}
}}"
  );
  Ok(conf)
}

/// Generate nginx conf for a resource
/// Return a tuple of (conf type, conf content)
/// conf type is either NginxConfKind::Site or NginxConfKind::Stream
/// conf content is the nginx conf content
async fn resource_to_nginx_conf(
  client: &NanocldClient,
  nginx: &Nginx,
  name: &str,
  resource_proxy: &ResourceProxyRule,
) -> Result<(NginxConfKind, String), ErrorHint> {
  let conf = match &resource_proxy.rule {
    ProxyRule::Http(rule) => {
      let conf = gen_http_server_block(name, rule, client, nginx).await?;
      (NginxConfKind::Site, conf)
    }
    ProxyRule::Stream(rule) => {
      let conf = gen_stream_server_block(name, rule, client, nginx).await?;
      (NginxConfKind::Stream, conf)
    }
  };
  log::info!("Generation conf for {name}");
  log::info!("Config type: {}", conf.0);
  log::info!("Config content: \n{}", conf.1);
  Ok(conf)
}

/// Reload the proxy configuration
/// This function will reload the nginx configuration
pub(crate) async fn reload_config(
  client: &NanocldClient,
) -> Result<(), ErrorHint> {
  log::info!("Reloading proxy configuration");
  let exec = CargoExecConfig::<String> {
    cmd: Some(vec!["nginx".into(), "-s".into(), "reload".into()]),
    ..Default::default()
  };
  client
    .exec_cargo("proxy", exec, Some("system".into()))
    .await
    .map_err(|err| {
      ErrorHint::warning(format!("Unable to reload proxy: {err}"))
    })?;
  log::info!("Proxy configuration reloaded");
  Ok(())
}

/// Create a new resource configuration
/// This function will create a new configuration file for the given resource
/// and reload the nginx configuration
/// The resource must be a ProxyRule
pub(crate) async fn create_resource_conf(
  client: &NanocldClient,
  nginx: &Nginx,
  resource: &nanocld_client::stubs::resource::Resource,
) -> Result<(), ErrorHint> {
  let proxy_rule = serialize_proxy_rule(resource)?;
  let (kind, conf) =
    resource_to_nginx_conf(client, nginx, &resource.name, &proxy_rule)
      .await
      .map_err(|err| {
        ErrorHint::warning(format!(
          "Unable to generate nginx conf for {name}: {err}",
          name = resource.name,
        ))
      })?;
  nginx.write_conf_file(&resource.name, &conf, &kind)?;
  Ok(())
}

/// List resources from nanocl daemon
/// This function will list all resources that contains the target key
/// in the watch list
/// The target key is the name of the cargo @ the namespace
/// The namespace is optional, if not provided, it will be set to "global"
pub(crate) async fn list_resource_by_cargo(
  name: &str,
  namespace: Option<String>,
  client: &NanocldClient,
) -> Result<Vec<nanocld_client::stubs::resource::Resource>, ErrorHint> {
  let namespace = namespace.unwrap_or("global".into());
  let target_key = format!("{name}.{namespace}");
  let query = ResourceQuery {
    contains: Some(serde_json::json!({ "Watch": [target_key] }).to_string()),
    kind: Some("ProxyRule".into()),
  };
  let resources = client.list_resource(Some(query)).await.map_err(|err| {
    ErrorHint::warning(format!(
      "Unable to list resources from nanocl daemon: {err}"
    ))
  })?;
  Ok(resources)
}

/// Sync resources from nanocl daemon
/// This function will remove all old configs and generate new ones
pub(crate) async fn sync_resources(
  client: &NanocldClient,
  nginx: &Nginx,
) -> Result<(), ErrorHint> {
  let query = ResourceQuery {
    kind: Some("ProxyRule".into()),
    ..Default::default()
  };
  let resources = client.list_resource(Some(query)).await.map_err(|err| {
    ErrorHint::warning(format!("Unable to list resources from nanocl: {err}"))
  })?;

  // remove old configs
  let _ = nginx.clear_conf();

  for resource in resources {
    if let Err(err) = create_resource_conf(client, nginx, &resource).await {
      err.print();
    }
  }
  reload_config(client).await?;
  Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
  use std::process::Output;

  use ntex::web;
  use ntex::web::error::BlockingError;

  pub fn before() {
    // Build a test env logger
    if std::env::var("LOG_LEVEL").is_err() {
      std::env::set_var("LOG_LEVEL", "nanocl-ctrl-proxy=info,warn,error,debug");
    }
    std::env::set_var("TEST", "true");
    let _ = env_logger::Builder::new()
      .parse_env("LOG_LEVEL")
      .is_test(true)
      .try_init();
  }

  pub(crate) async fn exec_nanocl(arg: &str) -> std::io::Result<Output> {
    let arg = arg.to_owned();
    web::block(move || {
      let mut cmd = std::process::Command::new("nanocl");
      let mut args = vec![];
      args.extend(arg.split(' ').collect::<Vec<&str>>());
      cmd.args(&args);
      let output = cmd.output()?;
      Ok::<_, std::io::Error>(output)
    })
    .await
    .map_err(|err| match err {
      BlockingError::Error(err) => err,
      BlockingError::Canceled => {
        std::io::Error::new(std::io::ErrorKind::Other, "Canceled")
      }
    })
  }
}
