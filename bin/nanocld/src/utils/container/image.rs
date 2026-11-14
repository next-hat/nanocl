use bollard_next::auth::DockerCredentials;
use futures::StreamExt;
use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocl_stubs::{
  generic::ImagePullPolicy,
  system::{
    EventActor, EventActorKind, EventKind, EventPartial, NativeEventAction,
  },
};

use crate::{
  models::{SecretDb, SystemState},
  repositories::generic::*,
  utils::secret::missing_secret_reference,
  vars,
};

fn map_pull_secret_error(name: &str, error: IoError) -> IoError {
  if error.context() == Some("SecretDb")
    && error.inner.kind() == std::io::ErrorKind::NotFound
    && matches!(
      error
        .inner
        .get_ref()
        .and_then(|source| source.downcast_ref::<diesel::result::Error>()),
      Some(diesel::result::Error::NotFound)
    )
  {
    missing_secret_reference(name)
  } else {
    error
  }
}

/// Describe image preparation failures without publishing registry responses or credentials.
pub(crate) fn pull_failure_diagnostic(error: &IoError) -> Option<String> {
  if let Some(detail) =
    crate::utils::secret::missing_reference_diagnostic(error)
  {
    return Some(detail);
  }
  if error.context() == Some("DownloadImage") {
    use bollard_next::errors::Error as DockerError;
    return Some(match error.inner.get_ref().and_then(|source| source.downcast_ref::<DockerError>()) {
      Some(DockerError::DockerResponseServerError { status_code, .. }) => {
        format!("Docker API returned HTTP status {status_code} while pulling an image")
      }
      Some(DockerError::RequestTimeoutError) => "Docker image pull request timed out".to_owned(),
      Some(DockerError::DockerStreamError { error }) => {
        let lower = error.to_ascii_lowercase();
        let reason = if lower.contains("manifest unknown") || lower.contains("manifest not found") {
          "the registry reported a missing image manifest"
        } else if lower.contains("unauthorized") || lower.contains("authentication required") || lower.contains("access denied") || lower.contains("denied: requested access") {
          "registry authentication or access appears to have been denied"
        } else if lower.contains("too many requests") || lower.contains("toomanyrequests") {
          "the registry appears to have rate limited the request"
        } else if lower.contains("timeout") || lower.contains("timed out") {
          "the registry request appears to have timed out"
        } else if lower.contains("no such host") || lower.contains("temporary failure in name resolution") {
          "registry hostname resolution appears to have failed"
        } else {
          "Docker reported an image download failure; inspect Docker logs for details"
        };
        reason.to_owned()
      },
      _ => "Unable to download an image through Docker; inspect Docker logs for details".to_owned(),
    });
  }
  match error.context() {
    Some("ParseName") => Some("Invalid container image name".to_owned()),
    Some("GetCredentials") => Some(
      "Unable to read image-pull credentials from the configured secret"
        .to_owned(),
    ),
    Some("SecretDb") => {
      Some("Unable to read a secret from the database".to_owned())
    }
    _ => None,
  }
}

/// Get the docker credentials to authenticate with the registry from the secret
///
async fn get_credentials(
  secret: Option<String>,
  state: &SystemState,
) -> IoResult<Option<DockerCredentials>> {
  Ok(match secret {
    Some(secret) => {
      let secret = SecretDb::read_by_pk(&secret, &state.inner.pool)
        .await
        .map_err(|error| map_pull_secret_error(&secret, error))?;
      serde_json::from_value::<DockerCredentials>(secret.data)
        .map(Some)
        .map_err(|err| err.map_err_context(|| "GetCredentials"))?
    }
    None => None,
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::utils::secret::{
    missing_reference_diagnostic, missing_reference_name,
  };

  #[test]
  fn pull_secret_lookup_maps_only_missing_database_rows() {
    let name = "registry\n\u{1b}[31m\"key";
    let missing =
      *diesel::result::Error::NotFound.map_err_context(|| "SecretDb");
    let mapped = map_pull_secret_error(name, missing);
    assert_eq!(missing_reference_name(&mapped), Some(name));
    assert_eq!(
      missing_reference_diagnostic(&mapped).as_deref(),
      Some(r#"Secret "registry\n\u{1b}[31m\"key" does not exist"#)
    );

    for error in [
      IoError::not_found("Pool", "pool lookup failed"),
      IoError::not_found("SecretDb", "unrelated missing file"),
      IoError::invalid_data("SecretDb", "database failure"),
    ] {
      let context = error.context().map(str::to_owned);
      let kind = error.inner.kind();
      let message = error.inner.to_string();
      let mapped = map_pull_secret_error(name, error);
      assert_eq!(mapped.context(), context.as_deref());
      assert_eq!(mapped.inner.kind(), kind);
      assert_eq!(mapped.inner.to_string(), message);
      assert!(missing_reference_diagnostic(&mapped).is_none());
    }
  }
}

/// Emit an event for the download status
///
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
///
pub fn parse_name(name: &str) -> IoResult<(String, String)> {
  let image_info: Vec<&str> = name.split(':').collect();
  if image_info.len() != 2 {
    return Err(IoError::invalid_input("ParseName", "Invalid image name"));
  }
  let image_name = image_info[0].to_ascii_lowercase();
  let image_tag = image_info[1].to_ascii_lowercase();
  Ok((image_name, image_tag))
}

/// Download the container image depending on the policy
///
pub async fn download<A>(
  image: &str,
  secret: Option<String>,
  policy: ImagePullPolicy,
  actor: &A,
  state: &SystemState,
) -> IoResult<()>
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
    Some(bollard_next::query_parameters::CreateImageOptions {
      from_image: Some(name.clone()),
      tag: Some(tag.clone()),
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
        let err = *err.map_err_context(|| "DownloadImage");
        let note = pull_failure_diagnostic(&err).unwrap_or_else(|| {
          "Docker reported an image download failure".to_owned()
        });
        emit_download_status(
          event_actor.clone(),
          event_related_actor.clone(),
          &format!("Failed to pull image {name}:{tag}: {note}"),
          NativeEventAction::Downloading,
          EventKind::Error,
          None,
          state,
        );
        return Err(err);
      }
      Ok(chunk) => chunk,
    };
    emit_download_status(
      event_actor.clone(),
      event_related_actor.clone(),
      &format!("Pulling image {name}:{tag}"),
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
    &format!("Pulled image {name}:{tag}"),
    NativeEventAction::Download,
    EventKind::Normal,
    None,
    state,
  );
  Ok(())
}
