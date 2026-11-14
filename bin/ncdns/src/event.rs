use std::{collections::BTreeMap, future::Future, time::Duration};

use futures::{
  Stream, StreamExt,
  future::{Either, LocalBoxFuture, pending, select},
};
use ntex::rt;

use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocl_utils::versioning;
use nanocld_client::NanocldClient;
use nanocld_client::stubs::resource_kind::{
  ResourceKindPartial, ResourceKindSpec,
};
use nanocld_client::stubs::system::Event;
use nanocld_client::stubs::{
  dns::ResourceDnsRule,
  generic::{GenericClause, GenericFilter},
};

use crate::{models::SystemState, utils, vars};

const RETRY_DELAY: Duration = Duration::from_secs(2);
const MAX_RECOVERY_RETRY_DELAY: Duration = Duration::from_secs(30);

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

async fn load_rules(
  client: &NanocldClient,
) -> IoResult<BTreeMap<String, ResourceDnsRule>> {
  const PAGE_SIZE: usize = 100;
  let mut rules = BTreeMap::new();
  let mut last_key = None;
  loop {
    let mut query = GenericFilter::new()
      .r#where("kind", GenericClause::Eq(vars::RULE_KEY.to_owned()))
      .limit(PAGE_SIZE);
    query.order_by = Some(vec!["key asc".to_owned()]);
    if let Some(key) = last_key {
      query = query.r#where("key", GenericClause::Gt(key));
    }
    let page = client.list_resource(Some(&query)).await?;
    let count = page.len();
    last_key = page
      .last()
      .map(|resource| resource.spec.resource_key.clone());
    for resource in page {
      let rule =
        serde_json::from_value(resource.spec.data).map_err(|error| {
          error.map_err_context(|| "DNS recovery declaration")
        })?;
      rules.insert(resource.spec.resource_key, rule);
    }
    if count < PAGE_SIZE {
      return Ok(rules);
    }
  }
}

async fn recover_rules(state: &SystemState) -> IoResult<()> {
  let needs_bootstrap = !state.rules.lock().await.bootstrapped;
  if needs_bootstrap {
    // Callbacks may complete while the inventory is fetched. Their accepted
    // declarations and deletion markers take precedence over this snapshot.
    let rules = load_rules(&state.client).await?;
    state.rules.lock().await.bootstrap(rules);
  }
  // Daemon requests must not hold the callback lock. Recheck each generation
  // before applying so a completed PUT/DELETE always wins over this snapshot.
  let snapshot = state.rules.lock().await.declarations.clone();
  let mut failure = None;
  for (key, (generation, rule)) in snapshot {
    let resolved = utils::resolve_rule(&rule, &state.client).await;
    let rules = state.rules.lock().await;
    if !rules.is_current(&key, generation) {
      continue;
    }
    let scope = match resolved {
      Ok(scope) => scope,
      Err(error) => {
        log::warn!("event::recovery: rule {key}: {error}");
        if resolution_needs_retry(&error) {
          failure.get_or_insert(error);
        }
        continue;
      }
    };
    if let Err(error) = state.dnsmasq.apply_rule(&key, scope).await {
      log::warn!("event::recovery: rule {key}: {error}");
      failure.get_or_insert(error);
    }
  }
  // Scan without the callback lock, then hold it only for one removal. A PUT
  // accepted during resolution or the scan must not be pruned as stale.
  for key in state.dnsmasq.rule_keys().await? {
    let rules = state.rules.lock().await;
    if rules.declarations.contains_key(&key) {
      continue;
    }
    if let Err(error) = state.dnsmasq.remove_rule(&key).await {
      log::warn!("event::recovery: stale rule {key}: {error}");
      failure.get_or_insert(error);
    }
  }
  match failure {
    Some(error) => Err(error),
    None => Ok(()),
  }
}

fn resolution_needs_retry(error: &IoError) -> bool {
  use std::io::ErrorKind;

  if let Some(http) = error
    .inner
    .get_ref()
    .and_then(|cause| cause.downcast_ref::<nanocl_error::http::HttpError>())
  {
    // HttpError conversion uses InvalidData for server errors as well.
    return http.status.is_server_error()
      || http.status == ntex::http::StatusCode::REQUEST_TIMEOUT
      || http.status == ntex::http::StatusCode::TOO_MANY_REQUESTS;
  }
  !matches!(
    (error.inner.kind(), error.context.as_deref()),
    (ErrorKind::NotFound | ErrorKind::InvalidInput, _)
      | (ErrorKind::InvalidData, Some("Network gateway"))
  )
}

/// Retry operational failures without abandoning the subscribed event stream.
/// The timer remains local to this subscription and is cancelled on EOF/error.
async fn consume_events<S, E, R, RF>(
  mut stream: S,
  mut recover: R,
  retry_delay: Duration,
  max_retry_delay: Duration,
) where
  S: Stream<Item = Result<Event, E>> + Unpin,
  E: std::fmt::Display,
  R: FnMut() -> RF,
  RF: Future<Output = IoResult<()>>,
{
  // An already-ready timer performs the first recovery immediately on every
  // subscription, before waiting for any daemon event (including reconnects).
  let mut retry: LocalBoxFuture<'static, ()> = Box::pin(async {});
  let mut next_delay = retry_delay;
  loop {
    // Retain the timer when ordinary events arrive and poll it first, so a busy
    // stream cannot postpone recovery indefinitely.
    match select(&mut retry, stream.next()).await {
      Either::Left(_) => {}
      Either::Right((Some(Ok(event)), _))
        if event.is_process_inventory_sync() => {}
      Either::Right((Some(Ok(_)), _)) => continue,
      Either::Right((Some(Err(error)), _)) => {
        log::warn!("event::loop: {error}");
        break;
      }
      Either::Right((None, _)) => break,
    }
    match recover().await {
      Ok(()) => {
        retry = Box::pin(pending());
        next_delay = retry_delay;
      }
      Err(error) => {
        log::warn!(
          "event::loop: DNS recovery failed: {error}; retrying in {} seconds",
          next_delay.as_secs()
        );
        retry = Box::pin(ntex::time::sleep(next_delay));
        next_delay = next_delay.saturating_mul(2).min(max_retry_delay);
      }
    }
  }
}

async fn ensure_loop(state: &SystemState) {
  let client = &state.client;
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
      Ok(stream) => {
        log::info!("event::loop: subscribed to nanocld events");
        consume_events(
          stream,
          || recover_rules(state),
          RETRY_DELAY,
          MAX_RECOVERY_RETRY_DELAY,
        )
        .await;
      }
    }
    log::warn!("event::loop: retrying in {} seconds", RETRY_DELAY.as_secs());
    ntex::time::sleep(RETRY_DELAY).await;
  }
}

/// Register the resource kind and recover declarations on startup/reconnect.
pub(crate) fn spawn(state: &SystemState) {
  let state = state.clone();
  rt::Arbiter::new().handle().spawn(async move {
    rt::spawn(async move {
      ensure_loop(&state).await;
      rt::Arbiter::current().stop();
    });
  });
}

#[cfg(test)]
mod tests {
  use nanocld_client::stubs::system::{
    EventActorKind, EventKind, NativeEventAction,
  };

  use super::*;

  fn state_sync_event() -> Event {
    serde_json::from_value(serde_json::json!({
      "Key": "00000000-0000-0000-0000-000000000000",
      "CreatedAt": "2026-01-01T00:00:00",
      "ExpiresAt": "2026-01-01T00:00:00",
      "ReportingNode": "node",
      "ReportingController": "nanocl.io/core",
      "Kind": "normal",
      "Action": NativeEventAction::Update.to_string(),
      "Reason": "state_sync",
      "Actor": {"Kind": EventActorKind::Process}
    }))
    .unwrap()
  }

  #[test]
  fn only_completed_generic_process_sync_triggers_recovery() {
    let mut event = state_sync_event();
    assert!(event.is_process_inventory_sync());
    event.kind = EventKind::Error;
    assert!(!event.is_process_inventory_sync());
    event.kind = EventKind::Normal;
    event.actor.as_mut().unwrap().key = Some("container-id".into());
    assert!(!event.is_process_inventory_sync());
    event.actor.as_mut().unwrap().key = None;
    event.reason = "another_update".into();
    assert!(!event.is_process_inventory_sync());
    event.reason = "state_sync".into();
    event.action = NativeEventAction::Start.to_string();
    assert!(!event.is_process_inventory_sync());
  }

  #[test]
  fn resolution_retry_distinguishes_missing_selectors_and_operational_errors() {
    use nanocl_error::{http::HttpError, http_client::HttpClientError};
    use ntex::http::StatusCode;

    // Preserve the exact conversion path used by named-network resolution.
    let missing: IoError =
      HttpClientError::HttpError(HttpError::not_found("deleted network"))
        .map_err_context(|| "Unable to inspect network private on node local")
        .into();
    for error in [
      missing,
      HttpError::bad_request("invalid selector").into(),
      IoError::not_found("Network", "deleted"),
      IoError::invalid_input("Network", "All is unsupported"),
      IoError::invalid_data("Network gateway", "no IPAM gateway"),
    ] {
      assert!(!resolution_needs_retry(&error), "{error}");
    }

    for error in [
      HttpError::new(StatusCode::SERVICE_UNAVAILABLE, "restart").into(),
      HttpError::new(StatusCode::REQUEST_TIMEOUT, "retry").into(),
      HttpError::new(StatusCode::TOO_MANY_REQUESTS, "retry").into(),
      IoError::invalid_data("Network", "invalid resolved address"),
      IoError::invalid_data("Unable to inspect network", "malformed response"),
      IoError::without_context(std::io::Error::new(
        std::io::ErrorKind::ConnectionReset,
        "daemon restarted",
      )),
    ] {
      assert!(resolution_needs_retry(&error), "{error}");
    }
  }

  #[ntex::test]
  async fn failed_recovery_consumes_events_until_stream_error() {
    let mut ordinary = state_sync_event();
    ordinary.reason = "another_update".into();
    let events = vec![
      Ok(ordinary.clone()),
      Ok(state_sync_event()),
      Ok(ordinary),
      Err(IoError::other("Stream", "disconnected")),
      Ok(state_sync_event()),
    ];
    let consumed = std::cell::Cell::new(0);
    let recoveries = std::cell::Cell::new(0);
    consume_events(
      futures::stream::iter(events).inspect(|_| {
        consumed.set(consumed.get() + 1);
      }),
      || {
        recoveries.set(recoveries.get() + 1);
        futures::future::ready(Err(IoError::other("Recovery", "unavailable")))
      },
      Duration::from_secs(3600),
      Duration::from_secs(3600),
    )
    .await;
    assert_eq!(recoveries.get(), 2);
    assert_eq!(consumed.get(), 4);
  }

  #[test]
  fn subscription_recovers_immediately_without_stream_events() {
    use futures::FutureExt;

    let recoveries = std::cell::Cell::new(0);
    for subscription in 1..=2 {
      let consuming = consume_events(
        futures::stream::pending::<Result<Event, IoError>>(),
        || {
          recoveries.set(recoveries.get() + 1);
          futures::future::ready(Ok(()))
        },
        RETRY_DELAY,
        MAX_RECOVERY_RETRY_DELAY,
      );
      // One poll performs recovery and then waits on the eventless stream.
      assert!(consuming.now_or_never().is_none());
      assert_eq!(recoveries.get(), subscription);
    }
  }

  #[ntex::test]
  async fn recovery_timer_retries_without_another_stream_event() {
    let (release, released) = futures::channel::oneshot::channel();
    let release = std::cell::RefCell::new(Some(release));
    let recoveries = std::cell::Cell::new(0);
    let stream = Box::pin(futures::stream::once(async {
      released.await.unwrap();
      let mut event = state_sync_event();
      event.reason = "another_update".into();
      Ok::<_, IoError>(event)
    }));
    ntex::time::timeout(
      Duration::from_secs(1),
      consume_events(
        stream,
        || {
          recoveries.set(recoveries.get() + 1);
          futures::future::ready(if recoveries.get() == 1 {
            Err(IoError::other("Recovery", "temporarily unavailable"))
          } else {
            release.borrow_mut().take().unwrap().send(()).unwrap();
            Ok(())
          })
        },
        Duration::from_millis(1),
        Duration::from_millis(4),
      ),
    )
    .await
    .unwrap();
    assert_eq!(recoveries.get(), 2);
  }
}
