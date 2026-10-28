use futures::StreamExt;
use ntex::rt;

use nanocl_error::io::{IoError, IoResult};

use nanocld_client::{
  NanocldClient,
  stubs::system::{
    EventActorKind, EventCondition, EventKind, NativeEventAction, ObjPsStatus,
  },
};

use crate::models::GenericProcessStatus;

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

pub async fn get_process_status(
  object_name: &str,
  key: &str,
  client: &NanocldClient,
) -> IoResult<ObjPsStatus> {
  let res = client
    .send_get(&format!("/{object_name}/{key}/inspect"), None::<String>)
    .await?;
  Ok(
    NanocldClient::res_json::<GenericProcessStatus>(res)
      .await?
      .status,
  )
}

pub async fn wait_process_state(
  key: &str,
  kind: EventActorKind,
  action: Vec<NativeEventAction>,
  client: &NanocldClient,
) -> IoResult<rt::JoinHandle<IoResult<()>>> {
  let condition = EventCondition {
    actor_key: Some(key.to_owned()),
    actor_kind: Some(kind),
    kind: vec![EventKind::Normal, EventKind::Error],
    action,
    ..Default::default()
  };
  let mut stream = client.watch_events(Some(vec![condition.clone()])).await?;
  let key = key.to_owned();
  let fut = rt::spawn(async move {
    while let Some(event) = stream.next().await {
      let event = event?;
      // A failed transition can use a different action from its completion.
      if event.kind == EventKind::Error
        && event.actor.as_ref().is_some_and(|actor| {
          actor.key.as_deref() == Some(key.as_str())
            && Some(&actor.kind) == condition.actor_kind.as_ref()
        })
      {
        return Err(IoError::interrupted(
          "Error",
          &event.note.unwrap_or_default(),
        ));
      }
      if event.kind == EventKind::Normal && condition == event {
        return Ok(());
      }
    }
    Err(IoError::interrupted(
      "wait_process_state",
      &format!("Event stream ended before the requested completion for {key}"),
    ))
  });
  Ok(fut)
}

/// Statefile replacement waits for the removed incarnation, never an older
/// run or a successor that reused its name. Start itself acknowledges dispatch.
pub async fn wait_job_deletion(
  key: &str,
  incarnation: uuid::Uuid,
  run: Option<uuid::Uuid>,
  client: &NanocldClient,
) -> IoResult<rt::JoinHandle<IoResult<()>>> {
  let condition = EventCondition {
    actor_key: Some(key.to_owned()),
    actor_kind: Some(EventActorKind::Job),
    kind: vec![EventKind::Normal, EventKind::Error],
    action: vec![NativeEventAction::Destroy],
    ..Default::default()
  };
  let mut stream = client.watch_events(Some(vec![condition.clone()])).await?;
  let incarnation = incarnation.to_string();
  let run = run.map(|key| key.to_string());
  Ok(rt::spawn(async move {
    while let Some(event) = stream.next().await {
      let event = event?;
      if !event
        .actor
        .as_ref()
        .and_then(|actor| actor.attributes.as_ref())
        .is_some_and(|attributes| {
          attributes
            .get("IncarnationKey")
            .and_then(serde_json::Value::as_str)
            == Some(incarnation.as_str())
            && attributes.get("RunKey").and_then(serde_json::Value::as_str)
              == run.as_deref()
        })
      {
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
  }))
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
    conditions: web::types::Json<Value>,
  ) -> web::HttpResponse {
    assert_eq!(conditions[0]["ActorKey"], "backup");
    assert_eq!(conditions[0]["ActorKind"], "Job");
    assert_eq!(conditions[0]["Action"], json!(["destroy"]));
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
    let waiter =
      wait_job_deletion("backup", uuid::Uuid::from_u128(1), run, &client)
        .await?;
    tokio::time::timeout(std::time::Duration::from_secs(5), waiter)
      .await
      .expect("Job deletion did not settle after the event stream")
      .expect("Job deletion waiter panicked")
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
}
