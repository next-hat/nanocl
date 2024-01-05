use std::str::FromStr;

use futures_util::StreamExt;

use nanocl_error::io::{FromIo, IoError, IoResult};

use vpnkitrc::stubs::*;

use nanocl_utils::logger;
use nanocld_client::NanocldClient;
use nanocld_client::stubs::system::{Event, EventActorKind, NativeEventAction};
use nanocld_client::stubs::resource::Resource;
use nanocld_client::stubs::proxy::{
  ResourceProxyRule, ProxyRule, ProxyStreamProtocol, ProxyRuleStream,
};

mod version;

/// Convert a Resource to a ProxyRule if the `Kind` is `ProxyRule`.
fn resource_to_proxy_rule(
  resource: &Resource,
) -> std::io::Result<ResourceProxyRule> {
  serde_json::from_value::<ResourceProxyRule>(resource.spec.data.clone())
    .map_err(|err| {
      std::io::Error::new(
        std::io::ErrorKind::InvalidData,
        format!("Unable to deserialize proxy rule: {err}"),
      )
    })
}

/// Apply vpnkit rule
async fn apply_rule(port: &VpnKitPort, vpnkit_client: &VpnKitRc) {
  if let Some(VpnKitProtocol::UNIX) = port.proto {
    log::info!(
      "Forwarding  {} - {} -> {}",
      port.proto.clone().unwrap_or_default(),
      port.out_path.clone().unwrap_or_default(),
      port.in_path.clone().unwrap_or_default(),
    );
    if let Err(err) = vpnkit_client.expose_pipe_path(port).await {
      log::error!("Error while creating the forwaring rule: {err}");
    }
  } else {
    log::info!(
      "Forwarding  {}  - {}:{} -> {}:{}",
      port.proto.clone().unwrap_or_default(),
      port.out_ip.clone().unwrap_or_default(),
      port.out_port.unwrap_or_default(),
      port.in_ip.clone().unwrap_or_default(),
      port.in_port.unwrap_or_default(),
    );
    if let Err(err) = vpnkit_client.expose_port(port).await {
      log::error!("Error while creating the forwaring rule: {err}");
    }
  }
}

/// Remove vpnkit rule
async fn remove_rule(port: &VpnKitPort, vpnkit_client: &VpnKitRc) {
  if let Some(VpnKitProtocol::UNIX) = port.proto {
    log::info!(
      "Backwarding {} - {} -> {}",
      port.proto.clone().unwrap_or_default(),
      port.out_path.clone().unwrap_or_default(),
      port.in_path.clone().unwrap_or_default(),
    );
    if let Err(err) = vpnkit_client.unexpose_pipe_path(port).await {
      log::error!("Error while removing the forwaring rule: {err}");
    }
  } else {
    log::info!(
      "Backwarding {}  - {}:{} -> {}:{}",
      port.proto.clone().unwrap_or_default(),
      port.out_ip.clone().unwrap_or_default(),
      port.out_port.unwrap_or_default(),
      port.in_ip.clone().unwrap_or_default(),
      port.in_port.unwrap_or_default(),
    );
    if let Err(err) = vpnkit_client.unexpose_port(port).await {
      log::error!("Error while removing the forwaring rule: {err}");
    }
  }
}

/// Convert a `ProxyRuleStream` to a `VpnKitPort`
fn rule_stream_to_vpnkit_port(rule_stream: &ProxyRuleStream) -> VpnKitPort {
  VpnKitPort {
    proto: match rule_stream.protocol {
      ProxyStreamProtocol::Tcp => Some(VpnKitProtocol::TCP),
      ProxyStreamProtocol::Udp => Some(VpnKitProtocol::UDP),
    },
    out_ip: Some("0.0.0.0".into()),
    out_port: Some(rule_stream.port.into()),
    in_ip: Some("127.0.0.1".into()),
    in_port: Some(rule_stream.port.into()),
    ..Default::default()
  }
}

/// Handle event from the nanocl daemon.
/// It's watching for ProxyRule events and apply the rules to vpnkit.
async fn on_event(
  event: &Event,
  nanocl_client: &NanocldClient,
  vpnkit_client: &VpnKitRc,
) -> IoResult<()> {
  let action = NativeEventAction::from_str(&event.action)?;
  let Some(actor) = event.actor.clone() else {
    return Ok(());
  };
  let actor_kind = &actor.kind;
  if actor_kind != &EventActorKind::Resource {
    return Ok(());
  }
  match action {
    NativeEventAction::Create | NativeEventAction::Update => {
      let key = actor.key.unwrap_or_default();
      let resource = nanocl_client.inspect_resource(&key).await?;
      let r_proxy_rule = resource_to_proxy_rule(&resource)?;
      for rule in r_proxy_rule.rules.into_iter() {
        if let ProxyRule::Stream(stream) = rule {
          match stream.network.as_str() {
            "Public" | "All" => {}
            _ => continue,
          }
          let port = rule_stream_to_vpnkit_port(&stream);
          apply_rule(&port, vpnkit_client).await;
        }
      }
    }
    NativeEventAction::Delete => {
      let attributes = actor.attributes.unwrap_or_default();
      let spec = attributes.get("Spec").cloned().ok_or_else(|| {
        IoError::invalid_data("Resource spec", "attribute not found")
      })?;
      let resource = serde_json::from_value::<ResourceProxyRule>(spec)
        .map_err(|err| err.map_err_context(|| "ncproxy.io/rule"))?;
      for rule in resource.rules.into_iter() {
        if let ProxyRule::Stream(stream) = rule {
          match stream.network.as_str() {
            "Public" | "All" => {}
            _ => continue,
          }
          let port = rule_stream_to_vpnkit_port(&stream);
          remove_rule(&port, vpnkit_client).await;
        }
      }
    }
    // Ignore other events
    _ => {}
  }
  Ok(())
}

#[ntex::main]
async fn main() -> std::io::Result<()> {
  logger::enable_logger("ncvpnkit");
  log::info!(
    "ncvpnkit_{}_{}_v{}:{}",
    version::ARCH,
    version::CHANNEL,
    version::VERSION,
    version::COMMIT_ID
  );
  let user_home = match std::env::var("USER_HOME") {
    Err(err) => {
      log::error!("Unable to get USER_HOME env variable: {err}");
      std::process::exit(1);
    }
    Ok(user_home) => user_home,
  };
  let vpnkit_client = VpnKitRc::connect_uds("/run/host-services/backend.sock");
  let nanocl_client = NanocldClient::connect_with_unix_default();
  let proxy_default = vec![
    VpnKitPort {
      proto: Some(VpnKitProtocol::TCP),
      out_ip: Some("0.0.0.0".into()),
      out_port: Some(80),
      in_ip: Some("127.0.0.1".into()),
      in_port: Some(80),
      ..Default::default()
    },
    VpnKitPort {
      proto: Some(VpnKitProtocol::TCP),
      out_ip: Some("0.0.0.0".into()),
      out_port: Some(443),
      in_ip: Some("127.0.0.1".into()),
      in_port: Some(443),
      ..Default::default()
    },
  ];
  let nanocld_unix_default = VpnKitPort {
    proto: Some(VpnKitProtocol::UNIX),
    out_path: Some(format!("{user_home}/.nanocl/run/nanocl.sock")),
    in_path: Some("/run/guest-services/nanocl/nanocl.sock".into()),
    ..Default::default()
  };
  loop {
    log::info!("Subscribing to nanocl daemon events..");
    match nanocl_client.watch_events().await {
      Err(err) => {
        log::warn!("Unable to Subscribe to nanocl daemon events: {err}");
      }
      Ok(mut stream) => {
        log::info!("Subscribed to nanocl daemon events");
        for port in proxy_default.iter() {
          apply_rule(port, &vpnkit_client).await;
        }
        apply_rule(&nanocld_unix_default, &vpnkit_client).await;
        while let Some(event) = stream.next().await {
          let event = match event {
            Err(err) => {
              log::warn!("Unable to get event: {err}");
              continue;
            }
            Ok(event) => event,
          };
          if let Err(err) =
            on_event(&event, &nanocl_client, &vpnkit_client).await
          {
            log::error!("{err}");
          }
        }
      }
    }
    log::warn!("Retrying to subscribe in 2 seconds");
    ntex::time::sleep(std::time::Duration::from_secs(2)).await;
  }
}
