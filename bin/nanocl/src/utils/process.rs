use futures::StreamExt;
use ntex::rt;
use nanocl_error::io::{IoError, IoResult};
use nanocld_client::{
  stubs::system::{
    EventActorKind, EventCondition, EventKind, NativeEventAction,
  },
  NanocldClient,
};

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
