use nanocl_error::io::{FromIo, IoError, IoResult};

use nanocld_client::stubs::dns::ResourceDnsRule;
use nanocld_client::stubs::generic::{
  GenericClause, GenericFilter, NetworkKind,
};
use nanocld_client::NanocldClient;

use crate::dnsmasq::Dnsmasq;

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

/// Get network address of given network
async fn get_network_addr(
  network: &NetworkKind,
  client: &NanocldClient,
) -> IoResult<String> {
  let addr = match network {
    NetworkKind::Local => "127.0.0.1".to_owned(),
    NetworkKind::Public => get_host_addr(client).await?,
    NetworkKind::Internal => get_bridge_addr(client).await?,
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
  client
    .restart_process("cargo", "ndns", Some("system"))
    .await?;
  Ok(())
}

pub(crate) async fn update_entries(
  key: &str,
  dns_rule: &ResourceDnsRule,
  dnsmasq: &Dnsmasq,
  client: &NanocldClient,
) -> IoResult<()> {
  let filter = GenericFilter::new()
    .r#where("kind", GenericClause::Eq("DnsRule".to_owned()))
    .r#where("key", GenericClause::Ne(key.to_owned()))
    .r#where(
      "data",
      GenericClause::Contains(
        serde_json::json!({ "Network": dns_rule.network }),
      ),
    );
  let resources = client.list_resource(Some(&filter)).await.map_err(|err| {
    err.map_err_context(|| "Unable to list resources from nanocl daemon")
  })?;
  log::debug!("utils::update_entries: {} resources", resources.len());
  let mut entries = dns_rule.entries.clone();
  for resource in resources {
    let mut dns_rule = serde_json::from_value::<ResourceDnsRule>(
      resource.spec.data,
    )
    .map_err(|err| err.map_err_context(|| "Unable to serialize the DnsRule"))?;
    entries.append(&mut dns_rule.entries);
  }
  let listen_address = get_network_addr(&dns_rule.network, client).await?;
  let mut file_content =
    format!("bind-dynamic\nlisten-address={listen_address}\n");
  for entry in &entries {
    let ip_address = match &entry.ip_address {
      NetworkKind::Local => "127.0.0.1".to_owned(),
      NetworkKind::Public => get_host_addr(client).await?,
      NetworkKind::Internal => get_bridge_addr(client).await?,
      NetworkKind::Other(ip) => ip.to_string(),
      NetworkKind::All => {
        return Err(IoError::invalid_input(
          "Network",
          "All network is not supported",
        ))
      }
    };
    let entry = &format!("address=/{}/{}", entry.name, ip_address);
    file_content += &format!("{entry}\n");
    log::debug!("utils::update_entries: {entry}");
  }
  dnsmasq
    .write_config(&dns_rule.network.to_string(), &file_content)
    .await?;
  Ok(())
}

pub(crate) async fn remove_entries(
  dns_rule: &ResourceDnsRule,
  dnsmasq: &Dnsmasq,
  client: &NanocldClient,
) -> IoResult<()> {
  let content = dnsmasq.read_config(&dns_rule.network.to_string()).await?;
  log::debug!("{}", content);
  let mut file_content = String::new();
  let lines = content.lines();
  let listen_address = get_network_addr(&dns_rule.network, client).await?;
  let empty_entries = format!("listen-address={listen_address}\n");
  for line in lines {
    let mut found = false;
    for entry in &dns_rule.entries {
      if line.starts_with(&format!("address=/{}/", entry.name)) {
        found = true;
        log::debug!("Found {}", line);
        break;
      }
    }
    if !found {
      file_content.push_str(&format!("{line}\n"));
    }
  }
  log::debug!("{}", file_content);
  if file_content == empty_entries {
    dnsmasq.remove_config(&dns_rule.network.to_string()).await?;
    return Ok(());
  }
  dnsmasq
    .write_config(&dns_rule.network.to_string(), &file_content)
    .await?;
  Ok(())
}

#[cfg(test)]
pub mod tests {
  pub use nanocl_utils::ntex::test_client::*;
  use nanocld_client::{ConnectOpts, NanocldClient};

  use crate::{dnsmasq, services, vars};

  // Before a test
  pub fn before() {
    // Build a test env logger
    std::env::set_var("TEST", "true");
  }

  // Generate a test server
  pub fn gen_default_test_client() -> TestClient {
    before();
    let dnsmasq = dnsmasq::Dnsmasq::new("/tmp/dnsmasq");
    dnsmasq
      .ensure()
      .expect("Expect to setup minimal dnsmasq config");
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })
    .expect("Failed to create a nanocl client");
    // Create test server
    let srv = ntex::web::test::server(move || {
      ntex::web::App::new()
        .state(dnsmasq.clone())
        .state(client.clone())
        .configure(services::ntex_config)
    });
    TestClient::new(srv, vars::VERSION)
  }
}
