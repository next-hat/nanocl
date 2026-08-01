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
  let mut stream = client
    .watch_events(Some(vec![EventCondition {
      actor_key: Some(key.to_owned()),
      actor_kind: Some(kind.clone()),
      kind: vec![EventKind::Normal, EventKind::Error],
      action,
      ..Default::default()
    }]))
    .await?;
  let key = key.to_owned();
  let fut = rt::spawn(async move {
    while let Some(event) = stream.next().await {
      let event = event?;
      let Some(actor) = &event.actor else {
        continue;
      };
      if event.kind == EventKind::Error
        && actor.key.clone().unwrap_or_default() == key
      {
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

#[cfg(test)]
mod tests {
  use super::resource_key;

  #[test]
  fn state_resource_keys_use_namespace_name_order() {
    assert_eq!(resource_key("jellyfin", "media").unwrap(), "media.jellyfin");
    assert_eq!(
      resource_key("website", "team.production").unwrap(),
      "team.production.website"
    );
  }
}
