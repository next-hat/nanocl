use nanocld_client::NanocldClient;
use nanocld_client::stubs::dns::ResourceDnsRule;

use nanocl_utils::io_error::{FromIo, IoResult, IoError};

use crate::dnsmasq::Dnsmasq;

/// Get gateway of given namespace
async fn get_namespace_addr(
  namespace: &str,
  client: &NanocldClient,
) -> IoResult<String> {
  let namespace = client.inspect_namespace(namespace).await.map_err(|err| {
    err.map_err_context(|| format!("Unable to inspect namespace {namespace}"))
  })?;
  let ipam = namespace.network.ipam.unwrap_or_default();
  let configs = ipam.config.unwrap_or_default();
  let config = configs.get(0).ok_or(IoError::not_fount(
    "NamespaceNetworkConfigs",
    "Unable to get index 0",
  ))?;
  config.gateway.clone().ok_or(IoError::not_fount(
    "NamespaceNetworkGateway",
    "Unable to get gateway",
  ))
}

/// Get public address of host
async fn get_host_addr(client: &NanocldClient) -> IoResult<String> {
  let info = client
    .info()
    .await
    .map_err(|err| err.map_err_context(|| "Unable to get host info"))?;
  Ok(info.host_gateway)
}

/// Get network address of given network
async fn get_network_addr(
  network: &str,
  client: &NanocldClient,
) -> IoResult<String> {
  let addr = match network {
    "Private" => "127.0.0.1".into(),
    "Public" => get_host_addr(client).await?,
    network if network.ends_with(".nsp") => {
      let network = network.trim_end_matches(".nsp");
      get_namespace_addr(network, client).await?
    }
    _ => {
      return Err(IoError::invalid_input(
        "Network",
        &format!("{network} is not supported"),
      ))
    }
  };
  Ok(addr)
}

/// Reload the dns service
/// TODO: use a better way to reload the service, we may have to move from dnsmasq to something else
pub(crate) async fn reload_service(client: &NanocldClient) -> IoResult<()> {
  client.stop_cargo("ndns", Some("system".into())).await?;
  client.start_cargo("ndns", Some("system".into())).await?;
  Ok(())
}

/// Convert a ResourceDnsRule into a dnsmasq config and write it to a file
pub(crate) async fn write_rule(
  name: &str,
  dns_rule: &ResourceDnsRule,
  dnsmasq: &Dnsmasq,
  client: &NanocldClient,
) -> IoResult<()> {
  let listen_address = get_network_addr(&dns_rule.network, client).await?;
  let mut entries = String::new();

  for entry in &dns_rule.entries {
    let ip_address = match entry.ip_address.as_str() {
      namespace if namespace.ends_with(".nsp") => {
        let namespace = namespace.trim_end_matches(".nsp");
        get_namespace_addr(namespace, client).await?
      }
      _ => entry.ip_address.clone(),
    };
    entries += &format!("address=/{}/{}\n", entry.name, ip_address);
  }

  let file_content = format!("listen-address={listen_address}\n{entries}");
  dnsmasq.write_config(name, &file_content).await?;
  Ok(())
}

#[cfg(test)]
pub mod tests {
  use nanocl_utils::logger;

  use crate::services;
  use crate::dnsmasq::Dnsmasq;

  // Before a test
  pub fn before() {
    // Build a test env logger
    std::env::set_var("TEST", "true");
    logger::enable_logger("ncdns");
  }

  // Generate a test server
  pub fn generate_server() -> ntex::web::test::TestServer {
    before();
    let dnsmasq = Dnsmasq::new("/tmp/dnsmasq");
    dnsmasq.ensure().unwrap();
    // Create test server
    ntex::web::test::server(move || {
      ntex::web::App::new()
        .state(dnsmasq.clone())
        .configure(services::ntex_config)
    })
  }
}
