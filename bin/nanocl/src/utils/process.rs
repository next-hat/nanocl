use std::future::Future;

use futures::StreamExt;

use nanocl_error::{
  http_client::HttpClientResult,
  io::{IoError, IoResult},
};

use nanocld_client::{
  NanocldClient,
  stubs::system::{
    Event, EventActorKind, EventCondition, EventKind, NativeEventAction,
  },
};

pub fn resource_key(name: &str, namespace: &str) -> IoResult<String> {
  nanocld_client::stubs::resource_key::ResourceKey::new(name, namespace)
    .map(|key| key.to_string())
    .map_err(|err| IoError::invalid_input("Resource key", &err.to_string()))
}

pub fn get_actor_kind(object_name: &str) -> EventActorKind {
  match object_name {
    "vms" => EventActorKind::Vm,
    "cargoes" => EventActorKind::Cargo,
    "jobs" => EventActorKind::Job,
    _ => {
      panic!("The developer trolled you with a wrong object name {object_name}")
    }
  }
}

/// Subscribe before admission, then accept only the operation returned by it.
/// Owning the stream in this future also closes it on rejection or cancellation.
pub async fn wait_process_operation<T: Into<Option<uuid::Uuid>>>(
  key: &str,
  kind: EventActorKind,
  action: NativeEventAction,
  client: &NanocldClient,
  request: impl Future<Output = HttpClientResult<T>>,
) -> HttpClientResult<()> {
  wait_process_operation_outcome(key, kind, action, client, request)
    .await
    .map(|_| ())
}

/// Return true only when the daemon authoritatively confirms no change.
pub async fn wait_process_operation_outcome<T: Into<Option<uuid::Uuid>>>(
  key: &str,
  kind: EventActorKind,
  action: NativeEventAction,
  client: &NanocldClient,
  request: impl Future<Output = HttpClientResult<T>>,
) -> HttpClientResult<bool> {
  // A name-only server condition can close the stream on a previous operation.
  let stream = client.watch_events(None).await?;
  wait_operation_outcome(stream, key, kind, action, request).await
}

async fn wait_operation_outcome<T: Into<Option<uuid::Uuid>>>(
  stream: impl futures::Stream<Item = nanocl_error::http::HttpResult<Event>> + Unpin,
  key: &str,
  kind: EventActorKind,
  action: NativeEventAction,
  request: impl Future<Output = HttpClientResult<T>>,
) -> HttpClientResult<bool> {
  let Some(operation) = request.await?.into() else {
    // An authoritative no-op has no operation event; release the subscription.
    return Ok(true);
  };
  wait_operation_events(stream, key, kind, action, operation).await?;
  Ok(false)
}

async fn wait_operation_events(
  mut stream: impl futures::Stream<Item = nanocl_error::http::HttpResult<Event>>
  + Unpin,
  key: &str,
  kind: EventActorKind,
  action: NativeEventAction,
  operation: uuid::Uuid,
) -> HttpClientResult<()> {
  while let Some(event) = stream.next().await {
    if let Some(result) =
      operation_event_result(&event?, key, &kind, &action, operation)
    {
      return result.map_err(Into::into);
    }
  }
  Err(IoError::interrupted(
    "Process operation",
    &format!(
      "Event stream ended before the requested completion for {key} (operation {operation})"
    ),
  ).into())
}

fn operation_event_result(
  event: &Event,
  key: &str,
  kind: &EventActorKind,
  action: &NativeEventAction,
  operation: uuid::Uuid,
) -> Option<IoResult<()>> {
  if !event.actor.as_ref().is_some_and(|actor| {
    actor.key.as_deref() == Some(key) && &actor.kind == kind
  }) || event
    .metadata
    .as_ref()
    .and_then(|metadata| metadata.get("Operation"))
    .and_then(serde_json::Value::as_str)
    .and_then(|value| value.parse::<uuid::Uuid>().ok())
    != Some(operation)
  {
    return None;
  }
  if event.reason == "operation_pending" {
    return Some(Err(IoError::interrupted(
      "Process operation pending",
      &format!(
        "Outcome unknown for {key} (operation {operation}); inspect its runtime and pending operation before retrying. {}",
        event
          .note
          .as_deref()
          .unwrap_or("Durable completion was not confirmed"),
      ),
    )));
  }
  if event.kind == EventKind::Error {
    return Some(Err(IoError::interrupted(
      "Process operation",
      event.note.as_deref().unwrap_or("Operation failed"),
    )));
  }
  // Admission and completion both use state_sync; their actions are distinct.
  if event.kind == EventKind::Normal && event.action == action.to_string() {
    return Some(Ok(()));
  }
  None
}

/// Statefile replacement waits for the removed incarnation, never an older
/// run or a successor that reused its name. Start itself acknowledges dispatch.
pub async fn wait_job_deletion(
  key: &str,
  incarnation: uuid::Uuid,
  run: Option<uuid::Uuid>,
  client: &NanocldClient,
  request: impl Future<Output = HttpClientResult<()>>,
) -> HttpClientResult<()> {
  // Server-side conditions close the stream on a matching name/action, before
  // this waiter can reject an old incarnation or run with that same name.
  let stream = client.watch_events(None).await?;
  request.await?;
  wait_job_deletion_events(stream, key.to_owned(), incarnation, run)
    .await
    .map_err(Into::into)
}

async fn wait_job_deletion_events(
  mut stream: impl futures::Stream<
    Item = nanocl_error::http::HttpResult<nanocld_client::stubs::system::Event>,
  > + Unpin,
  key: String,
  incarnation: uuid::Uuid,
  run: Option<uuid::Uuid>,
) -> IoResult<()> {
  let condition = EventCondition {
    actor_key: Some(key.clone()),
    actor_kind: Some(EventActorKind::Job),
    kind: vec![EventKind::Normal],
    action: vec![NativeEventAction::Destroy],
    ..Default::default()
  };
  let incarnation = incarnation.to_string();
  let run = run.map(|key| key.to_string());
  while let Some(event) = stream.next().await {
    let event = event?;
    if !event.actor.as_ref().is_some_and(|actor| {
      actor.key.as_deref() == Some(key.as_str())
        && actor.kind == EventActorKind::Job
        && actor.attributes.as_ref().is_some_and(|attributes| {
          attributes
            .get("IncarnationKey")
            .and_then(serde_json::Value::as_str)
            == Some(incarnation.as_str())
            && attributes.get("RunKey").and_then(serde_json::Value::as_str)
              == run.as_deref()
        })
    }) {
      continue;
    }
    if event.kind == EventKind::Error {
      return Err(IoError::interrupted(
        "Job deletion",
        &event.note.unwrap_or_default(),
      ));
    }
    if condition == event {
      return Ok(());
    }
  }
  Err(IoError::interrupted(
    "Job deletion",
    "Event stream ended before incarnation deletion",
  ))
}

#[cfg(test)]
mod tests {
  use nanocld_client::{ConnectOpts, NANOCLD_DEFAULT_VERSION};
  use ntex::web;
  use serde_json::{Value, json};

  use super::*;

  #[test]
  fn state_resource_keys_use_namespace_name_order() {
    assert_eq!(resource_key("jellyfin", "media").unwrap(), "media.jellyfin");
    assert_eq!(
      resource_key("website", "team.production").unwrap(),
      "team.production.website"
    );
  }

  #[test]
  fn operation_wait_matches_only_the_accepted_identity() {
    let operation = uuid::Uuid::from_u128(1);
    let mut value = deletion_event(operation, None, "normal", "start");
    value["Actor"]["Key"] = json!("global.api");
    value["Actor"]["Kind"] = json!("Cargo");
    value["Metadata"] = json!({"Operation": operation});
    let result = |value: Value| {
      operation_event_result(
        &serde_json::from_value::<Event>(value).unwrap(),
        "global.api",
        &EventActorKind::Cargo,
        &NativeEventAction::Start,
        operation,
      )
    };
    result(value.clone()).unwrap().unwrap();
    for (pointer, other) in [
      ("/Metadata/Operation", json!(uuid::Uuid::from_u128(2))),
      ("/Metadata/Operation", json!("invalid")),
      ("/Metadata", Value::Null),
      ("/Actor/Key", json!("global.other")),
      ("/Actor/Kind", json!("Vm")),
      ("/Action", json!("starting")),
      ("/Kind", json!("warning")),
    ] {
      let mut unrelated = value.clone();
      *unrelated.pointer_mut(pointer).unwrap() = other;
      assert!(result(unrelated).is_none(), "{pointer}");
    }
    value["Kind"] = json!("error");
    value["Action"] = json!("updating");
    let failure = result(value.clone()).unwrap().unwrap_err();
    assert!(failure.to_string().contains("Exact Job deletion failed"));
    value["Kind"] = json!("warning");
    value["Action"] = json!("operation_pending");
    value["Reason"] = json!("operation_pending");
    let pending = result(value.clone()).unwrap().unwrap_err();
    assert_eq!(pending.context(), Some("Process operation pending"));
    assert_eq!(pending.inner.kind(), std::io::ErrorKind::Interrupted);
    assert!(pending.to_string().contains("Outcome unknown"));
    assert!(pending.to_string().contains(&operation.to_string()));
    value["Metadata"]["Operation"] = json!(uuid::Uuid::from_u128(2));
    assert!(result(value).is_none());
  }

  #[test]
  fn operation_outcome_unchanged_drops_stream_without_polling() {
    struct DropSignal(std::rc::Rc<std::cell::Cell<bool>>);
    impl Drop for DropSignal {
      fn drop(&mut self) {
        self.0.set(true);
      }
    }
    let dropped = std::rc::Rc::new(std::cell::Cell::new(false));
    let signal = DropSignal(dropped.clone());
    let stream = futures::stream::poll_fn(
      move |_| -> std::task::Poll<
        Option<nanocl_error::http::HttpResult<Event>>,
      > {
        let _ = &signal;
        panic!("An unchanged request must not poll for an operation event");
      },
    );
    let unchanged = futures::executor::block_on(wait_operation_outcome(
      stream,
      "global.api",
      EventActorKind::Cargo,
      NativeEventAction::Start,
      async {
        assert!(
          !dropped.get(),
          "the stream must remain owned until the response"
        );
        Ok(None::<uuid::Uuid>)
      },
    ))
    .unwrap();
    assert!(unchanged);
    assert!(
      dropped.get(),
      "the no-op must release the event subscription"
    );
  }

  #[ntex::test]
  async fn operation_outcome_accepted_requires_matching_completion() {
    let operation = uuid::Uuid::from_u128(1);
    let mut value = deletion_event(operation, None, "normal", "start");
    value["Actor"]["Key"] = json!("global.api");
    value["Actor"]["Kind"] = json!("Cargo");
    value["Metadata"] = json!({"Operation": operation});
    let event = serde_json::from_value::<Event>(value).unwrap();
    let unchanged = wait_operation_outcome(
      futures::stream::iter(vec![Ok(event)]),
      "global.api",
      EventActorKind::Cargo,
      NativeEventAction::Start,
      async { Ok(Some(operation)) },
    )
    .await
    .unwrap();
    assert!(!unchanged);
    assert!(
      wait_operation_outcome(
        futures::stream::empty(),
        "global.api",
        EventActorKind::Cargo,
        NativeEventAction::Start,
        async { Ok(operation) },
      )
      .await
      .is_err()
    );
  }

  #[ntex::test]
  async fn operation_stream_ignores_predecessors_and_requires_completion() {
    let operation = uuid::Uuid::from_u128(1);
    let event = |id, kind, action| {
      let mut value = deletion_event(operation, None, kind, action);
      value["Actor"]["Key"] = json!("global.api");
      value["Actor"]["Kind"] = json!("Cargo");
      value["Metadata"] = json!({"Operation": id});
      serde_json::from_value::<Event>(value).unwrap()
    };
    let stale = uuid::Uuid::from_u128(2);
    wait_operation_events(
      futures::stream::iter(vec![
        Ok(event(stale, "normal", "start")),
        Ok(event(stale, "error", "start")),
        Ok(event(operation, "normal", "starting")),
        Ok(event(operation, "normal", "start")),
      ]),
      "global.api",
      EventActorKind::Cargo,
      NativeEventAction::Start,
      operation,
    )
    .await
    .unwrap();
    let error = wait_operation_events(
      futures::stream::iter(vec![Ok(event(stale, "normal", "start"))]),
      "global.api",
      EventActorKind::Cargo,
      NativeEventAction::Start,
      operation,
    )
    .await
    .unwrap_err();
    assert!(
      error
        .to_string()
        .contains("before the requested completion")
    );
    let error = wait_operation_events(
      futures::stream::iter(vec![Ok(event(operation, "error", "starting"))]),
      "global.api",
      EventActorKind::Cargo,
      NativeEventAction::Start,
      operation,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("Exact Job deletion failed"));
  }

  #[ntex::test]
  async fn operation_stream_state_sync_admission_is_not_completion() {
    let operation = uuid::Uuid::from_u128(1);
    for (kind, admission, completion) in [
      (
        EventActorKind::Cargo,
        NativeEventAction::Starting,
        NativeEventAction::Start,
      ),
      (
        EventActorKind::Cargo,
        NativeEventAction::Stopping,
        NativeEventAction::Stop,
      ),
      (
        EventActorKind::Cargo,
        NativeEventAction::Updating,
        NativeEventAction::Update,
      ),
      (
        EventActorKind::Cargo,
        NativeEventAction::Destroying,
        NativeEventAction::Destroy,
      ),
      (
        EventActorKind::Vm,
        NativeEventAction::Starting,
        NativeEventAction::Start,
      ),
      (
        EventActorKind::Vm,
        NativeEventAction::Updating,
        NativeEventAction::Update,
      ),
      (
        EventActorKind::Vm,
        NativeEventAction::Destroying,
        NativeEventAction::Destroy,
      ),
    ] {
      let event = |event_kind: &str, action: &NativeEventAction| {
        let mut value =
          deletion_event(operation, None, event_kind, &action.to_string());
        value["Actor"]["Key"] = json!("global.api");
        value["Actor"]["Kind"] = json!(kind);
        value["Metadata"] = json!({"Operation": operation});
        value["Reason"] = json!("state_sync");
        value["Note"] = json!("The admitted operation failed");
        serde_json::from_value::<Event>(value).unwrap()
      };
      let error = wait_operation_events(
        futures::stream::iter(vec![Ok(event("normal", &admission))]),
        "global.api",
        kind.clone(),
        completion.clone(),
        operation,
      )
      .await
      .unwrap_err();
      assert!(
        error
          .to_string()
          .contains("before the requested completion")
      );

      let error = wait_operation_events(
        futures::stream::iter(vec![
          Ok(event("normal", &admission)),
          Ok(event("error", &completion)),
        ]),
        "global.api",
        kind.clone(),
        completion.clone(),
        operation,
      )
      .await
      .unwrap_err();
      assert!(error.to_string().contains("The admitted operation failed"));

      wait_operation_events(
        futures::stream::iter(vec![
          Ok(event("normal", &admission)),
          Ok(event("normal", &completion)),
        ]),
        "global.api",
        kind.clone(),
        completion,
        operation,
      )
      .await
      .unwrap();
    }
  }

  #[ntex::test]
  async fn operation_stream_reports_pending_without_waiting_for_completion() {
    let operation = uuid::Uuid::from_u128(1);
    for kind in [EventActorKind::Cargo, EventActorKind::Vm] {
      let mut pending =
        deletion_event(operation, None, "warning", "operation_pending");
      pending["Actor"]["Key"] = json!("global.api");
      pending["Actor"]["Kind"] = json!(kind);
      pending["Metadata"] = json!({"Operation": operation});
      pending["Reason"] = json!("operation_pending");
      pending["Note"] = json!("Execution stopped without durable settlement");
      let mut stale = pending.clone();
      stale["Metadata"]["Operation"] = json!(uuid::Uuid::from_u128(2));
      let stream = futures::stream::iter(vec![
        Ok(serde_json::from_value::<Event>(stale).unwrap()),
        Ok(serde_json::from_value::<Event>(pending).unwrap()),
      ])
      .chain(futures::stream::pending());
      let error = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        wait_operation_events(
          stream,
          "global.api",
          kind,
          NativeEventAction::Start,
          operation,
        ),
      )
      .await
      .expect("pending receipt must not wait for a terminal event")
      .unwrap_err();
      assert!(error.to_string().contains("Outcome unknown"));
      assert!(error.to_string().contains(&operation.to_string()));
      assert!(error.to_string().contains("before retrying"));
    }
  }

  fn deletion_event(
    incarnation: uuid::Uuid,
    run: Option<uuid::Uuid>,
    kind: &str,
    action: &str,
  ) -> Value {
    json!({
      "Key": uuid::Uuid::from_u128(10),
      "CreatedAt": "2026-01-01T00:00:00",
      "ExpiresAt": "2026-01-01T01:00:00",
      "ReportingNode": "fixture",
      "ReportingController": "nanocl.io/core",
      "Kind": kind,
      "Action": action,
      "Reason": "Job deletion fixture",
      "Note": "Exact Job deletion failed",
      "Actor": {
        "Key": "backup", "Kind": "Job",
        "Attributes": {"IncarnationKey": incarnation, "RunKey": run},
      },
    })
  }

  async fn serve_deletion_events(
    request: web::HttpRequest,
    body: ntex::util::Bytes,
  ) -> web::HttpResponse {
    assert!(
      body.is_empty(),
      "Job deletion must use an unconditioned stream"
    );
    web::HttpResponse::Ok()
      .content_type("application/vdn.nanocl.raw-stream")
      .body(request.app_state::<String>().unwrap().clone())
  }

  async fn deletion_wait(
    events: Vec<Value>,
    run: Option<uuid::Uuid>,
  ) -> IoResult<()> {
    let body = events
      .into_iter()
      .map(|event| format!("{event}\n"))
      .collect::<String>();
    let server = web::test::server(async move |_| {
      web::App::new()
        .with_config(web::WebAppConfig::new().set_state(body.clone()))
        .service(
          web::resource(format!("/v{NANOCLD_DEFAULT_VERSION}/events/watch"))
            .route(web::post().to(serve_deletion_events)),
        )
    });
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: format!("http://localhost:{}", server.addr().port()),
      ..Default::default()
    })
    .unwrap();
    tokio::time::timeout(
      std::time::Duration::from_secs(5),
      wait_job_deletion(
        "backup",
        uuid::Uuid::from_u128(1),
        run,
        &client,
        async { Ok(()) },
      ),
    )
    .await
    .expect("Job deletion did not settle after the event stream")
    .map_err(Into::into)
  }

  #[ntex::test]
  async fn job_deletion_wait_rejects_old_incarnation_and_run_events() {
    let incarnation = uuid::Uuid::from_u128(1);
    let run = Some(uuid::Uuid::from_u128(2));
    let old_incarnation = uuid::Uuid::from_u128(3);
    let old_run = Some(uuid::Uuid::from_u128(4));
    deletion_wait(
      vec![
        deletion_event(old_incarnation, run, "error", "destroy"),
        deletion_event(incarnation, old_run, "error", "stop"),
        deletion_event(old_incarnation, run, "normal", "destroy"),
        deletion_event(incarnation, old_run, "normal", "destroy"),
        deletion_event(incarnation, run, "normal", "stop"),
        deletion_event(incarnation, run, "normal", "destroy"),
      ],
      run,
    )
    .await
    .unwrap();
  }

  #[ntex::test]
  async fn job_deletion_wait_requires_exact_completion_before_eof() {
    let run = Some(uuid::Uuid::from_u128(2));
    for ignored in [
      deletion_event(uuid::Uuid::from_u128(3), run, "normal", "destroy"),
      deletion_event(
        uuid::Uuid::from_u128(1),
        Some(uuid::Uuid::from_u128(4)),
        "normal",
        "destroy",
      ),
      deletion_event(uuid::Uuid::from_u128(1), run, "normal", "stop"),
    ] {
      let error = deletion_wait(vec![ignored], run).await.unwrap_err();
      assert!(error.to_string().contains("before incarnation deletion"));
    }
    let error = deletion_wait(
      vec![deletion_event(
        uuid::Uuid::from_u128(1),
        run,
        "error",
        "stop",
      )],
      run,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("Exact Job deletion failed"));
  }

  #[ntex::test]
  async fn job_deletion_wait_handles_never_started_incarnation() {
    let incarnation = uuid::Uuid::from_u128(1);
    deletion_wait(
      vec![
        deletion_event(
          incarnation,
          Some(uuid::Uuid::from_u128(2)),
          "error",
          "destroy",
        ),
        deletion_event(incarnation, None, "normal", "destroy"),
      ],
      None,
    )
    .await
    .unwrap();
  }
  #[ntex::test]
  async fn job_deletion_stream_requires_exact_actor_and_execution() {
    let incarnation = uuid::Uuid::from_u128(1);
    let run = Some(uuid::Uuid::from_u128(2));
    let mut wrong_key = deletion_event(incarnation, run, "error", "destroy");
    wrong_key["Actor"]["Key"] = json!("another-job");
    let mut wrong_kind = deletion_event(incarnation, run, "error", "stop");
    wrong_kind["Actor"]["Kind"] = json!("Cargo");
    let events = vec![
      wrong_key,
      wrong_kind,
      deletion_event(uuid::Uuid::from_u128(3), run, "normal", "destroy"),
      deletion_event(
        incarnation,
        Some(uuid::Uuid::from_u128(4)),
        "normal",
        "destroy",
      ),
      deletion_event(incarnation, run, "normal", "stop"),
      deletion_event(incarnation, run, "normal", "destroy"),
    ];
    let events = futures::stream::iter(events.into_iter().map(|event| {
      Ok(
        serde_json::from_value::<nanocld_client::stubs::system::Event>(event)
          .unwrap(),
      )
    }));
    wait_job_deletion_events(events, "backup".into(), incarnation, run)
      .await
      .unwrap();

    let event =
      serde_json::from_value(deletion_event(incarnation, run, "error", "stop"))
        .unwrap();
    let error = wait_job_deletion_events(
      futures::stream::iter(vec![Ok(event)]),
      "backup".into(),
      incarnation,
      run,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("Exact Job deletion failed"));

    let event = serde_json::from_value(deletion_event(
      incarnation,
      None,
      "normal",
      "destroy",
    ))
    .unwrap();
    wait_job_deletion_events(
      futures::stream::iter(vec![Ok(event)]),
      "backup".into(),
      incarnation,
      None,
    )
    .await
    .unwrap();

    let error = wait_job_deletion_events(
      futures::stream::empty(),
      "backup".into(),
      incarnation,
      run,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("before incarnation deletion"));
  }
}
