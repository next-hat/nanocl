use nanocld_client::NanocldClient;
use nanocl_utils::io_error::{IoResult, FromIo, IoError};

/// Import cargo types
use nanocld_client::stubs::cargo::{CargoInspect, CreateExecOptions};
/// Import resource types
use nanocld_client::stubs::resource::{ResourceQuery, ResourcePartial};
/// Import proxy types
use nanocld_client::stubs::proxy::{
  ProxyRule, StreamTarget, ProxyStreamProtocol, ProxyRuleHttp, CargoTarget,
  ProxyHttpLocation, ProxyRuleStream, LocationTarget, ResourceProxyRule,
};

use crate::nginx::{Nginx, NginxConfKind};

/// Serialize a ProxyRule
pub(crate) fn serialize_proxy_rule(
  resource: &ResourcePartial,
) -> IoResult<ResourceProxyRule> {
  let proxy_rule =
    serde_json::from_value::<ResourceProxyRule>(resource.config.to_owned())
      .map_err(|err| {
        err.map_err_context(|| "Unable to serialize ResourceProxyRule")
      })?;
  Ok(proxy_rule)
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
    .get(0)
    .ok_or(IoError::invalid_data("IpamConfig", "Unable to get index 0"))?;

  let ip_address = ipam_config
    .gateway
    .clone()
    .ok_or(IoError::invalid_data("IpamConfig", "Unable to get gateway"))?;

  Ok(ip_address)
}

async fn get_listen(
  network: &str,
  port: u16,
  client: &NanocldClient,
) -> IoResult<String> {
  match network {
    "Public" => Ok(format!("{port}")),
    "Internal" => Ok(format!("127.0.0.1:{port}")),
    network if network.ends_with(".nsp") => {
      let namespace = network.trim_end_matches(".nsp");
      let ip_address = get_namespace_addr(namespace, client).await?;
      Ok(format!("{ip_address}:{port}"))
    }
    _ => Err(IoError::invalid_data(
      "Network",
      &format!("network {}", network),
    )),
  }
}

fn create_cargo_upstream(
  kind: &NginxConfKind,
  port: u16,
  cargo: &CargoInspect,
  nginx: &Nginx,
) -> IoResult<String> {
  let ip_addresses = cargo
    .instances
    .iter()
    .map(|node_container| {
      let container = node_container.container.clone();
      let networks = container
        .network_settings
        .unwrap_or_default()
        .networks
        .unwrap_or_default();
      let network =
        networks
          .get(&cargo.namespace_name)
          .ok_or(IoError::invalid_data(
            "Networks",
            &format!("Unable to get network {}", &cargo.namespace_name),
          ))?;
      let ip_address =
        network.ip_address.clone().ok_or(IoError::invalid_data(
          "IpAddress",
          &format!("for cargo {}", &cargo.name),
        ))?;
      Ok::<_, IoError>(ip_address)
    })
    .collect::<Result<Vec<String>, IoError>>()?;
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
) -> IoResult<String> {
  let port = target.cargo_port;
  let (cargo_name, namespace) = extract_target_cargo(&target.cargo_key)?;
  let cargo = client
    .inspect_cargo(&cargo_name, Some(namespace.clone()))
    .await
    .map_err(|err| {
      err.map_err_context(|| format!("Unable to inspect cargo {cargo_name}"))
    })?;
  create_cargo_upstream(kind, port, &cargo, nginx)
}

fn extract_target_cargo(key: &str) -> IoResult<(String, String)> {
  let info = key.split('.').collect::<Vec<&str>>();
  if info.len() != 2 {
    return Err(IoError::invalid_data(
      "CargoKey",
      "Invalid cargo key expected <name>.<namespace>",
    ));
  }
  let namespace = info[1].to_owned();
  let name = info[0].to_owned();
  Ok((name, namespace))
}

async fn gen_unix_stream(path: &str, nginx: &Nginx) -> IoResult<String> {
  let upstream_key = format!("unix-{}", path.replace('/', "-"));
  let upstream = format!(
    "upstream {upstream_key} {{
  server unix:{path};
}}
",
    path = path
  );
  nginx.write_conf_file(&upstream_key, &upstream, &NginxConfKind::Site)?;
  Ok(upstream_key)
}

async fn gen_locations(
  location_rules: &Vec<ProxyHttpLocation>,
  client: &NanocldClient,
  nginx: &Nginx,
) -> IoResult<Vec<String>> {
  let mut locations = Vec::new();
  for rule in location_rules {
    let path = &rule.path;

    let version = match &rule.version {
      None => String::default(),
      Some(version) => format!("\n    proxy_http_version {};", version),
    };

    let headers = rule.headers.clone().unwrap_or_default().into_iter().fold(
      String::new(),
      |mut acc, elem| {
        acc += &format!("\n    proxy_set_header {elem};");
        acc
      },
    );

    match &rule.target {
      LocationTarget::Cargo(cargo_target) => {
        let upstream_key =
          gen_cargo_upstream(&NginxConfKind::Site, cargo_target, client, nginx)
            .await?;
        let location = format!(
          "
  location {path} {{{version}{headers}
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-Scheme $scheme;
    proxy_set_header X-Forwarded-Proto  $scheme;
    proxy_set_header X-Forwarded-For    $proxy_add_x_forwarded_for;
    proxy_set_header X-Real-IP          $remote_addr;
    proxy_pass http://{upstream_key};
  }}"
        );
        locations.push(location);
      }
      LocationTarget::Unix(unix) => {
        let upstream_key = gen_unix_stream(&unix.unix_path, nginx).await?;
        let location = format!(
          "location {path} {{{version}{headers}
    proxy_pass http://{upstream_key}/;
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-Scheme $scheme;
    proxy_set_header X-Forwarded-Proto  $scheme;
    proxy_set_header X-Forwarded-For    $proxy_add_x_forwarded_for;
    proxy_set_header X-Real-IP          $remote_addr;
  }}
  "
        );
        locations.push(location);
      }
      LocationTarget::Http(http_target) => {
        let url = http_target.url.clone();
        let location = match &http_target.redirect {
          Some(redirect) => {
            format!(
              "
  location {path} {{{version}{headers}
    return {redirect} {url};
  }}
"
            )
          }
          None => {
            format!(
              "
  location {path} {{{version}{headers}
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
  rule: &ProxyRuleHttp,
  client: &NanocldClient,
  nginx: &Nginx,
) -> IoResult<String> {
  let listen_http = get_listen(&rule.network, 80, client).await?;
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
    let listen_https = get_listen(&rule.network, 443, client).await?;
    format!(
      "
  listen {listen_https} http2 ssl;

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
  listen {listen_http};
{http_host}{ssl}{includes}
{locations}
}}\n",
  );
  Ok(conf)
}

async fn gen_stream_server_block(
  rule: &ProxyRuleStream,
  client: &NanocldClient,
  nginx: &Nginx,
) -> IoResult<String> {
  let port = rule.port;
  let mut listen = get_listen(&rule.network, port, client).await?;

  let upstream_key = match &rule.target {
    StreamTarget::Cargo(cargo_target) => {
      gen_cargo_upstream(&NginxConfKind::Stream, cargo_target, client, nginx)
        .await?
    }
    StreamTarget::Unix(unix) => gen_unix_stream(&unix.unix_path, nginx).await?,
    StreamTarget::Uri(_) => {
      return Err(IoError::invalid_input(
        "StreamTarget",
        "Uri is not supported yet sorry",
      ))
    }
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
    ssl_certificate      {certificate};
    ssl_certificate_key  {certificate_key};{ssl_dh_param}
          "
    )
  } else {
    String::default()
  };
  if rule.protocol == ProxyStreamProtocol::Udp {
    listen = format!("{} udp", listen);
  }
  let conf = format!(
    "
server {{
  listen {listen};
  proxy_pass {upstream_key};
{ssl}
}}
"
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
) -> IoResult<(NginxConfKind, String)> {
  let conf = match &resource_proxy.rules {
    ProxyRule::Http(rules) => {
      let mut conf = String::new();
      for rule in rules {
        conf += &gen_http_server_block(rule, client, nginx).await?;
      }
      (NginxConfKind::Site, conf)
    }
    ProxyRule::Stream(rules) => {
      let mut conf = String::new();

      for rule in rules {
        conf += &gen_stream_server_block(rule, client, nginx).await?;
      }

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
pub(crate) async fn reload_config(client: &NanocldClient) -> IoResult<()> {
  log::info!("Reloading proxy configuration");
  let exec = CreateExecOptions {
    cmd: Some(vec!["nginx".into(), "-s".into(), "reload".into()]),
    ..Default::default()
  };
  client
    .exec_cargo("nproxy", exec, Some("system".into()))
    .await
    .map_err(|err| {
      err.map_err_context(|| {
        "Unable to reload proxy on container nproxy.system.c"
      })
    })?;
  log::info!("Proxy configuration reloaded");
  Ok(())
}

/// Create a new resource configuration
/// This function will create a new configuration file for the given resource
/// and reload the nginx configuration
/// The resource must be a ProxyRule
pub(crate) async fn create_resource_conf(
  name: &str,
  proxy_rule: &ResourceProxyRule,
  client: &NanocldClient,
  nginx: &Nginx,
) -> IoResult<()> {
  let (kind, conf) =
    resource_to_nginx_conf(client, nginx, name, proxy_rule).await?;
  nginx.write_conf_file(name, &conf, &kind)?;
  Ok(())
}

/// Sync resources from nanocl daemon
/// This function will remove all old configs and generate new ones
/// TODO Make call this function from api endpoint
// pub(crate) async fn sync_resources(
//   client: &NanocldClient,
//   nginx: &Nginx,
// ) -> IoResult<()> {
//   let query = ResourceQuery {
//     kind: Some("ProxyRule".into()),
//     ..Default::default()
//   };
//   let resources = client.list_resource(Some(query)).await.map_err(|err| {
//     err.map_err_context(|| "Unable to list resources from nanocl")
//   })?;

//   // remove old configs
//   let _ = nginx.clear_conf();

//   for resource in resources {
//     let proxy_rule = serialize_proxy_rule(&resource.clone().into())?;
//     if let Err(err) =
//       create_resource_conf(&resource.name, &proxy_rule, client, nginx).await
//     {
//       log::warn!("{err}")
//     }
//   }
//   reload_config(client).await?;
//   Ok(())
// }

/// List resources from nanocl daemon
/// This function will list all resources that contains the target key
/// in the watch list
/// The target key is the name of the cargo @ the namespace
/// The namespace is optional, if not provided, it will be set to "global"
pub(crate) async fn list_resource_by_cargo(
  name: &str,
  namespace: Option<String>,
  client: &NanocldClient,
) -> IoResult<Vec<nanocld_client::stubs::resource::Resource>> {
  let namespace = namespace.unwrap_or("global".into());
  let target_key = format!("{name}.{namespace}");
  let query = ResourceQuery {
    contains: Some(serde_json::json!({ "Watch": [target_key] }).to_string()),
    kind: Some("ProxyRule".into()),
  };
  let resources = client.list_resource(Some(query)).await.map_err(|err| {
    err.map_err_context(|| "Unable to list resources from nanocl daemon")
  })?;
  Ok(resources)
}

#[cfg(test)]
pub(crate) mod tests {
  use std::process::Output;
  use ntex::web::error::BlockingError;

  use nanocl_utils::logger;

  use crate::services;
  use crate::nginx::Nginx;

  // Before a test
  pub fn before() {
    // Build a test env logger
    std::env::set_var("TEST", "true");
    logger::enable_logger("ncdproxy");
  }

  pub async fn exec_nanocl(arg: &str) -> std::io::Result<Output> {
    let arg = arg.to_owned();
    ntex::web::block(move || {
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

  pub fn generate_server() -> ntex::web::test::TestServer {
    before();
    let nginx = Nginx::new("/tmp/nginx");
    nginx.ensure().unwrap();
    // Create test server
    ntex::web::test::server(move || {
      ntex::web::App::new()
        .state(nginx.clone())
        .configure(services::ntex_config)
    })
  }
}
