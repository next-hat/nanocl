use nanocld_client::stubs::cargo::CargoInspect;

use crate::dnsmasq::Dnsmasq;
use crate::error::ErrorHint;

/// Generate cargo domains records
pub(crate) fn gen_cargo_domains(
  cargo: &CargoInspect,
) -> Result<Vec<(String, String)>, ErrorHint> {
  let mut domains = Vec::new();
  for instance in &cargo.instances {
    let id = instance.container.id.clone().unwrap_or_default();
    let networks = instance
      .container
      .network_settings
      .to_owned()
      .unwrap_or_default()
      .networks
      .unwrap_or_default();
    let network =
      networks
        .get(&cargo.namespace_name)
        .ok_or(ErrorHint::Warning(format!(
          "Unable to find network for container {id} of cargo {}",
          cargo.key,
        )))?;
    let name = instance
      .container
      .names
      .to_owned()
      .unwrap_or_default()
      .get(0)
      .ok_or(ErrorHint::Warning(format!(
        "Unable to find name for container {} for cargo {} in namespace {}",
        instance.container.id.to_owned().unwrap_or_default(),
        cargo.key,
        cargo.namespace_name,
      )))?
      .to_owned()
      .replace('/', "")
      .replace(['-', '_'], ".");
    let ip_address =
      network
        .ip_address
        .to_owned()
        .ok_or(ErrorHint::Warning(format!(
      "Unable to find ip address for container {} for cargo {} in namespace {}",
      instance.container.id.to_owned().unwrap_or_default(),
      cargo.key,
      cargo.namespace_name,
    )))?;
    let domain = format!("nanocl.{name}.local");
    println!("[DEBUG] Entry generated {domain}/{ip_address}");
    domains.push((domain, ip_address));
  }
  Ok(domains)
}

/// Restart dnsmask to apply the new configuration
pub(crate) async fn restart_dns_service(
  client: &nanocld_client::NanocldClient,
) -> Result<(), ErrorHint> {
  let cargo = "dns";
  let namespace = Some(String::from("system"));
  client
    .stop_cargo(cargo, namespace.to_owned())
    .await
    .map_err(|err| {
      ErrorHint::Warning(format!("unable to stop dns cargo got error: {err}"))
    })?;
  client.start_cargo(cargo, namespace).await.map_err(|err| {
    ErrorHint::Warning(format!("unable to start dns cargo got error: {err}"))
  })?;
  Ok(())
}

/// Get all cargos and create dns records
/// This function is called at startup
/// To ensure that no data is lost
pub(crate) async fn sync_daemon_state(
  client: &nanocld_client::NanocldClient,
  dnsmasq: &Dnsmasq,
) -> Result<(), ErrorHint> {
  println!("[INFO] Syncing daemon state");
  dnsmasq.clear_domains()?;
  let namespaces = client.list_namespace().await.map_err(|err| {
    ErrorHint::Warning(format!("unable to list namespaces got error: {err}"))
  })?;
  for namespace in namespaces {
    let cargos = client
      .list_cargo(Some(namespace.name.to_owned()))
      .await
      .map_err(|err| {
        ErrorHint::Warning(format!("unable to list cargos got error: {err}"))
      })?;
    for cargo in cargos {
      if cargo.name == "dns" && cargo.namespace_name == "system" {
        continue;
      }
      let cargo = client
        .inspect_cargo(&cargo.name, Some(namespace.name.to_owned()))
        .await
        .map_err(|err| {
          ErrorHint::Warning(format!(
            "unable to inspect cargo got error: {err}"
          ))
        })?;
      let domains = gen_cargo_domains(&cargo)?;
      dnsmasq.generate_domains_file(&cargo.key, &domains)?;
    }
  }
  restart_dns_service(client).await?;
  println!("[INFO] Daemon state synced");
  Ok(())
}
