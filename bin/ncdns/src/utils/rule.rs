use nanocl_error::io::{FromIo, IoResult, IoError};

use nanocld_client::{
  NanocldClient,
  stubs::{
    generic::{GenericFilter, GenericClause},
    dns::ResourceDnsRule,
  },
};
use ntex::rt::System;

use crate::models::{Dnsmasq, SystemStateRef};

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
  let config = configs.get(0).ok_or(IoError::not_found(
    "NamespaceNetworkConfigs",
    "Unable to get index 0",
  ))?;
  config.gateway.clone().ok_or(IoError::not_found(
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
  client.restart_cargo("ndns", Some("system")).await?;
  Ok(())
}

pub(crate) async fn update_entries(
  key: &str,
  dns_rule: &ResourceDnsRule,
  state: &SystemStateRef,
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
  let resources =
    state
      .client
      .list_resource(Some(&filter))
      .await
      .map_err(|err| {
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
  let listen_address =
    get_network_addr(&dns_rule.network, &state.client).await?;
  let mut file_content =
    format!("bind-dynamic\nlisten-address={listen_address}\n");
  for entry in &entries {
    let ip_address = match entry.ip_address.as_str() {
      namespace if namespace.ends_with(".nsp") => {
        let namespace = namespace.trim_end_matches(".nsp");
        get_namespace_addr(namespace, &state.client).await?
      }
      _ => entry.ip_address.clone(),
    };
    let entry = &format!("address=/{}/{}", entry.name, ip_address);
    file_content += &format!("{entry}\n");
    log::debug!("utils::update_entries: {entry}");
  }
  state
    .dnsmasq
    .write_config(&dns_rule.network, &file_content)
    .await?;
  Ok(())
}

pub(crate) async fn remove_entries(
  dns_rule: &ResourceDnsRule,
  state: &SystemStateRef,
) -> IoResult<()> {
  let content = state.dnsmasq.read_config(&dns_rule.network).await?;
  println!("{}", content);
  let mut file_content = String::new();
  let lines = content.lines();
  let listen_address =
    get_network_addr(&dns_rule.network, &state.client).await?;
  let empty_entries = format!("listen-address={listen_address}\n");
  for line in lines {
    let mut found = false;
    for entry in &dns_rule.entries {
      if line.starts_with(&format!("address=/{}/", entry.name)) {
        found = true;
        println!("Found {}", line);
        break;
      }
    }
    if !found {
      file_content.push_str(&format!("{line}\n"));
    }
  }
  println!("{}", file_content);
  if file_content == empty_entries {
    state.dnsmasq.remove_config(&dns_rule.network).await?;
    return Ok(());
  }
  state
    .dnsmasq
    .write_config(&dns_rule.network, &file_content)
    .await?;
  Ok(())
}
