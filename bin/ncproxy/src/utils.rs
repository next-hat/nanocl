use futures::StreamExt;
use nanocld_client::bollard_next;
use nanocld_client::NanocldClient;
use nanocl_utils::io_error::{IoResult, FromIo, IoError};

/// Import cargo types
use nanocld_client::stubs::cargo::{CargoInspect, CreateExecOptions};
/// Import resource types
use nanocld_client::stubs::resource::{ResourceQuery, ResourcePartial};
/// Import proxy types
use nanocld_client::stubs::proxy::{
  ProxyRule, StreamTarget, ProxyStreamProtocol, ProxyRuleHttp, UpstreamTarget,
  ProxyHttpLocation, ProxyRuleStream, LocationTarget, ResourceProxyRule,
};
use nanocld_client::stubs::vm::VmInspect;

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
  let mut ip_addresses = Vec::new();

  for node_container in cargo.instances.iter() {
    let container = node_container.container.clone();
    let networks = container
      .network_settings
      .unwrap_or_default()
      .networks
      .unwrap_or_default();

    let network = networks.get(&cargo.namespace_name);

    let Some(network) = network else {
      log::warn!("empty ip address for cargo {}", &cargo.name);
      log::warn!("Instance is unhealthy, skipping");
      continue;
    };
    let Some(ip_address) = network.ip_address.clone() else {
      log::warn!("empty ip address for cargo {}", &cargo.name);
      log::warn!("Instance is unhealthy, skipping");
      continue;
    };
    if ip_address.is_empty() {
      log::warn!("empty ip address for cargo {}", &cargo.name);
      log::warn!("Instance is unhealthy, skipping");
      continue;
    }
    ip_addresses.push(ip_address);
  }
  if ip_addresses.is_empty() {
    return Err(IoError::invalid_data(
      "CargoInspect",
      &format!("Unable to get ip addresses for cargo {}", &cargo.name),
    ));
  }
  log::debug!("ip_addresses: {:?}", ip_addresses);
  let upstream_key = format!("cargo-{}-{}", cargo.key, port);
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

fn create_vm_upstream(
  kind: &NginxConfKind,
  port: u16,
  vm: &VmInspect,
  nginx: &Nginx,
) -> IoResult<String> {
  let mut ip_addresses = Vec::new();

  for node_container in vm.instances.iter() {
    let networks = node_container
      .network_settings
      .clone()
      .unwrap_or_default()
      .networks
      .unwrap_or_default();

    let network = networks.get(&vm.namespace_name);

    let Some(network) = network else {
      log::warn!("empty ip address for vm {}", &vm.name);
      log::warn!("Instance is unhealthy, skipping");
      continue;
    };
    let Some(ip_address) = network.ip_address.clone() else {
      log::warn!("empty ip address for cargo {}", &vm.name);
      log::warn!("Instance is unhealthy, skipping");
      continue;
    };
    if ip_address.is_empty() {
      log::warn!("empty ip address for cargo {}", &vm.name);
      log::warn!("Instance is unhealthy, skipping");
      continue;
    }
    ip_addresses.push(ip_address);
  }
  log::debug!("ip_addresses: {:?}", ip_addresses);
  let upstream_key = format!("vm-{}-{}", vm.key, port);
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

async fn gen_upstream(
  kind: &NginxConfKind,
  target: &UpstreamTarget,
  client: &NanocldClient,
  nginx: &Nginx,
) -> IoResult<String> {
  let port = target.port;
  let (target_name, target_namespace, target_kind) =
    extract_upstream_target(&target.key)?;

  match target_kind.as_str() {
    "c" => {
      let cargo = client
        .inspect_cargo(&target_name, Some(target_namespace.clone()))
        .await
        .map_err(|err| {
          err.map_err_context(|| {
            format!("Unable to inspect cargo {target_name}")
          })
        })?;
      create_cargo_upstream(kind, port, &cargo, nginx)
    }
    "v" => {
      let vm = client
        .inspect_vm(&target_name, Some(target_namespace.clone()))
        .await
        .map_err(|err| {
          err.map_err_context(|| format!("Unable to inspect vm {target_name}"))
        })?;
      create_vm_upstream(kind, port, &vm, nginx)
    }
    _ => Err(IoError::invalid_data(
      "UpstreamTarget",
      &format!("Unknown Kind {}", target_kind),
    )),
  }
}

fn extract_upstream_target(key: &str) -> IoResult<(String, String, String)> {
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
      LocationTarget::Upstream(upstream_target) => {
        let Ok(upstream_key) =
          gen_upstream(&NginxConfKind::Site, upstream_target, client, nginx)
            .await
        else {
          log::warn!("Unable to generate cargo upstream for location rule {:?} got error", rule);
          continue;
        };
        let disable_logging =
          if upstream_target.disable_logging.unwrap_or_default() {
            "access_log off;"
          } else {
            ""
          };
        let location = format!(
          "
  location {path} {{{version}{headers}
    proxy_set_header Host $host;
    proxy_set_header X-Forwarded-Scheme $scheme;
    proxy_set_header X-Forwarded-Proto  $scheme;
    proxy_set_header X-Forwarded-For    $proxy_add_x_forwarded_for;
    proxy_set_header X-Real-IP          $remote_addr;
    proxy_pass http://{upstream_key}{};
    {disable_logging}
  }}",
          upstream_target.path.clone().unwrap_or("".into())
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
    Some(domain) => format!(
      "  server_name {domain};\n  if ($host != {domain}) {{ return 404; }}\n",
      domain = domain
    ),
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
    let mut base = format!(
      "
  listen {listen_https} http2 ssl;

  if ($scheme != https) {{
    return 301 https://$host$request_uri;
  }}

  ssl_certificate      {certificate};
  ssl_certificate_key  {certificate_key};{ssl_dh_param}
"
    );

    if let Some(certificate_client) = &ssl.certificate_client {
      base += &format!("  ssl_client_certificate {certificate_client};\n");
    }

    if let Some(client_verification) = &ssl.verify_client {
      base += &format!(
        "  ssl_verify_client {};\n",
        if *client_verification { "on" } else { "off" }
      );
    }

    base
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
    StreamTarget::Upstream(cargo_target) => {
      gen_upstream(&NginxConfKind::Stream, cargo_target, client, nginx).await?
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
    let mut base = format!(
      "
    ssl_certificate      {certificate};
    ssl_certificate_key  {certificate_key};{ssl_dh_param}
"
    );

    if let Some(certificate_client) = &ssl.certificate_client {
      base += &format!("  ssl_client_certificate {certificate_client};\n");
    }

    if let Some(client_verification) = &ssl.verify_client {
      base += &format!(
        "  ssl_verify_client {};\n",
        if *client_verification { "on" } else { "off" }
      );
    }

    base
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
) -> IoResult<()> {
  let mut http_conf = String::new();
  let mut stream_conf = String::new();
  for rule in resource_proxy.rules.iter() {
    match rule {
      ProxyRule::Http(rule) => {
        http_conf += &gen_http_server_block(rule, client, nginx).await?;
      }
      ProxyRule::Stream(rule) => {
        stream_conf += &gen_stream_server_block(rule, client, nginx).await?;
      }
    }
  }
  log::info!("Generation conf for {name}");
  if !http_conf.is_empty() {
    nginx.write_conf_file(name, &http_conf, &NginxConfKind::Site)?;
    log::info!("HTTP config generated:\n{http_conf}");
  }
  if !stream_conf.is_empty() {
    nginx.write_conf_file(name, &stream_conf, &NginxConfKind::Stream)?;
    log::info!("Stream config generated:\n{stream_conf}");
  }
  Ok(())
}

/// Reload the proxy configuration
/// This function will reload the nginx configuration
pub(crate) async fn reload_config(client: &NanocldClient) -> IoResult<()> {
  log::info!("Reloading proxy configuration");

  let exec_options = CreateExecOptions {
    cmd: Some(vec!["nginx".into(), "-s".into(), "reload".into()]),
    ..Default::default()
  };

  let start_res = client
    .create_exec("nproxy", exec_options, Some("system".into()))
    .await
    .map_err(|err| err.map_err_context(|| "Unable to reload proxy configs"))?;

  let mut start_stream = client
    .start_exec(
      &start_res.id,
      bollard_next::exec::StartExecOptions::default(),
    )
    .await
    .map_err(|err| err.map_err_context(|| "Unable to reload proxy configs"))?;

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
        log::info!("Proxy configuration reloaded");
        return Ok(());
      }

      Err(IoError::invalid_data("nproxy reload", &output))
    }
    None => Ok(()),
  }
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
  resource_to_nginx_conf(client, nginx, name, proxy_rule).await?;
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
  let target_key = format!("{name}.{namespace}.c");
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
    logger::enable_logger("ncproxy");
  }

  pub async fn exec_nanocl(arg: &str) -> std::io::Result<Output> {
    let arg = arg.to_owned();
    ntex::web::block(move || {
      let mut cmd = std::process::Command::new("cargo");
      let mut args = vec!["make", "run-cli"];
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
