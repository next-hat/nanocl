use ntex::rt;
use futures::StreamExt;

use nanocl_error::io::{IoError, IoResult};

use nanocld_client::{
  NanocldClient,
  stubs::{
    generic::GenericNspQuery,
    system::{
      EventActorKind, EventCondition, EventKind, NativeEventAction, ObjPsStatus,
    },
  },
};

use crate::models::GenericProcessStatus;

pub fn gen_key(name: &str, namespace: Option<String>) -> String {
  match namespace {
    Some(ns) => format!("{}/{}", ns, name),
    None => name.to_owned(),
  }
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
  name: &str,
  namespace: Option<String>,
  client: &NanocldClient,
) -> IoResult<ObjPsStatus> {
  let res = client
    .send_get(
      &format!("/{object_name}/{name}/inspect"),
      Some(GenericNspQuery::new(namespace.as_deref())),
    )
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
  let mut stream = client
    .watch_events(Some(vec![EventCondition {
      actor_key: Some(key.to_owned()),
      actor_kind: Some(kind.clone()),
      kind: vec![EventKind::Normal, EventKind::Error],
      action,
      ..Default::default()
    }]))
    .await?;
  let fut = rt::spawn(async move {
    while let Some(event) = stream.next().await {
      let event = event?;
      if event.kind == EventKind::Error {
        return Err(IoError::interrupted(
          "Error",
          &event.note.unwrap_or_default(),
        ));
      }
    }
    Ok::<_, IoError>(())
  });
  Ok(fut)
}
