use bollard_next::auth::DockerCredentials;
use futures::StreamExt;
use nanocl_error::http::{HttpError, HttpResult};
use nanocl_stubs::{
  generic::ImagePullPolicy,
  system::{
    EventActor, EventActorKind, EventKind, EventPartial, NativeEventAction,
  },
};

use crate::{
  models::{SecretDb, SystemState},
  repositories::generic::*,
  vars,
};

/// Get the docker credentials to authenticate with the registry from the secret
async fn get_credentials(
  secret: Option<String>,
  state: &SystemState,
) -> HttpResult<Option<DockerCredentials>> {
  Ok(match secret {
    Some(secret) => {
      let secret = SecretDb::read_by_pk(&secret, &state.inner.pool).await?;
      serde_json::from_value::<DockerCredentials>(secret.data)
        .map(Some)
        .map_err(|err| HttpError::bad_request(err.to_string()))?
    }
    None => None,
  })
}

fn emit_download_status(
  actor: Option<EventActor>,
  related: Option<EventActor>,
  note: &str,
  action: NativeEventAction,
  kind: EventKind,
  metadata: Option<serde_json::Value>,
  state: &SystemState,
) {
  let event = EventPartial {
    reporting_controller: vars::CONTROLLER_NAME.to_owned(),
    reporting_node: state.inner.config.hostname.clone(),
    action: action.to_string(),
    reason: "state_sync".to_owned(),
    kind,
    actor,
    related,
    metadata,
    note: Some(note.to_owned()),
  };
  state.spawn_emit_event(event);
}

/// Get the image name and tag from a string
pub fn parse_name(name: &str) -> HttpResult<(String, String)> {
  let image_info: Vec<&str> = name.split(':').collect();
  if image_info.len() != 2 {
    return Err(HttpError::bad_request("Missing tag in image name"));
  }
  let image_name = image_info[0].to_ascii_lowercase();
  let image_tag = image_info[1].to_ascii_lowercase();
  Ok((image_name, image_tag))
}

/// Download the image
pub async fn download<A>(
  image: &str,
  secret: Option<String>,
  policy: ImagePullPolicy,
  actor: &A,
  state: &SystemState,
) -> HttpResult<()>
where
  A: Into<EventActor> + Clone,
{
  match policy {
    ImagePullPolicy::Always => {}
    ImagePullPolicy::IfNotPresent => {
      if state.inner.docker_api.inspect_image(image).await.is_ok() {
        return Ok(());
      }
    }
    ImagePullPolicy::Never => {
      return Ok(());
    }
  }
  let credentials = get_credentials(secret, state).await?;
  let (name, tag) = parse_name(image)?;
  let mut stream = state.inner.docker_api.create_image(
    Some(bollard_next::image::CreateImageOptions {
      from_image: name.clone(),
      tag: tag.clone(),
      ..Default::default()
    }),
    None,
    credentials,
  );
  let event_actor = Some(EventActor {
    key: Some(image.to_owned()),
    kind: EventActorKind::ContainerImage,
    attributes: None,
  });
  let event_related_actor: Option<EventActor> = Some(actor.clone().into());
  while let Some(chunk) = stream.next().await {
    let chunk = match chunk {
      Err(err) => {
        emit_download_status(
          event_actor.clone(),
          event_related_actor.clone(),
          &format!("{err}"),
          NativeEventAction::Downloading,
          EventKind::Error,
          None,
          state,
        );
        return Err(err.into());
      }
      Ok(chunk) => chunk,
    };
    emit_download_status(
      event_actor.clone(),
      event_related_actor.clone(),
      &format!("{name}:{tag}"),
      NativeEventAction::Downloading,
      EventKind::Normal,
      Some(serde_json::json!({
        "state": chunk,
      })),
      state,
    );
  }
  emit_download_status(
    event_actor.clone(),
    event_related_actor.clone(),
    &format!("{name}:{tag}"),
    NativeEventAction::Download,
    EventKind::Normal,
    None,
    state,
  );
  Ok(())
}
