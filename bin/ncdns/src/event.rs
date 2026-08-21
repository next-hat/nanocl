use std::time::Duration;

use futures::StreamExt;
use ntex::rt;

use nanocl_error::io::IoResult;

use nanocl_utils::versioning;

use nanocld_client::stubs::resource_kind::{
  ResourceKindPartial, ResourceKindSpec,
};

use nanocld_client::NanocldClient;

use crate::{dnsmasq::Dnsmasq, vars};

const RETRY_DELAY: Duration = Duration::from_secs(2);

pub(super) async fn ensure_self_config(client: &NanocldClient) -> IoResult<()> {
  let formatted_version = versioning::format_version(vars::VERSION);
  let resource_kind = ResourceKindPartial {
    name: vars::RULE_KEY.to_owned(),
    version: format!("v{formatted_version}"),
    metadata: None,
    data: ResourceKindSpec {
      schema: None,
      url: Some("unix:///run/nanocl/dns.sock".to_owned()),
    },
  };
  if client
    .inspect_resource_kind_version(&resource_kind.name, &resource_kind.version)
    .await
    .is_ok()
  {
    return Ok(());
  }
  client.create_resource_kind(&resource_kind).await?;
  Ok(())
}

#[cfg(test)]
fn dns_rule_actor_key(
  kind: &nanocld_client::stubs::system::EventKind,
  action: &str,
  actor: Option<&nanocld_client::stubs::system::EventActor>,
) -> Option<String> {
  use nanocld_client::stubs::system::{EventActorKind, EventKind};

  if *kind != EventKind::Normal
    || !matches!(action, "create" | "update" | "destroy")
  {
    return None;
  }
  let actor = actor.filter(|actor| actor.kind == EventActorKind::Resource)?;
  let resource_kind = actor
    .attributes
    .as_ref()?
    .get("Kind")?
    .as_str()?
    .split('/')
    .take(2)
    .collect::<Vec<_>>()
    .join("/");
  if resource_kind != vars::RULE_KEY {
    return None;
  }
  actor.key.clone()
}

async fn watch_loop(client: &NanocldClient, _dnsmasq: &Dnsmasq) {
  loop {
    if let Err(err) = ensure_self_config(client).await {
      log::warn!("event::loop: unable to ensure resource kind: {err}");
      ntex::time::sleep(RETRY_DELAY).await;
      continue;
    }
    log::info!("event::loop: subscribing to nanocld events");
    match client.watch_events(None).await {
      Err(err) => {
        log::warn!("event::loop: {err}");
      }
      Ok(mut stream) => {
        log::info!("event::loop: subscribed to nanocld events");
        while let Some(_event) = stream.next().await {
          // Should only update if containers or VMs are being updated.
        }
      }
    }
    log::warn!("event::loop: retrying in 2 seconds");
    ntex::time::sleep(RETRY_DELAY).await;
  }
}

/// Spawn new thread with event loop to watch for nanocld events
pub(crate) fn spawn(client: &NanocldClient, dnsmasq: &Dnsmasq) {
  let client = client.clone();
  let dnsmasq = dnsmasq.clone();
  rt::Arbiter::new().handle().spawn(async move {
    rt::spawn(async move {
      watch_loop(&client, &dnsmasq).await;
      rt::Arbiter::current().stop();
    });
  });
}

#[cfg(test)]
mod tests {
  use nanocld_client::stubs::system::{EventActor, EventActorKind, EventKind};

  use super::dns_rule_actor_key;

  fn actor(kind: &str) -> EventActor {
    EventActor {
      key: Some("rule-a".to_owned()),
      kind: EventActorKind::Resource,
      attributes: Some(serde_json::json!({ "Kind": kind })),
    }
  }

  #[test]
  fn committed_dns_resource_events_acknowledge_their_key() {
    for action in ["create", "update", "destroy"] {
      assert_eq!(
        dns_rule_actor_key(
          &EventKind::Normal,
          action,
          Some(&actor("ncdns.io/rule"))
        )
        .as_deref(),
        Some("rule-a")
      );
    }
  }

  #[test]
  fn unrelated_or_failed_events_do_not_acknowledge_overrides() {
    assert!(
      dns_rule_actor_key(
        &EventKind::Normal,
        "create",
        Some(&actor("ncproxy.io/rule"))
      )
      .is_none()
    );
    assert!(
      dns_rule_actor_key(
        &EventKind::Error,
        "create",
        Some(&actor("ncdns.io/rule"))
      )
      .is_none()
    );
    assert!(
      dns_rule_actor_key(
        &EventKind::Normal,
        "starting",
        Some(&actor("ncdns.io/rule"))
      )
      .is_none()
    );
  }
}
