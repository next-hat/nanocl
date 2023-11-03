use nanocl_error::http::HttpError;
use nanocld_client::stubs::secret::Secret;
use serde::{Serialize, Deserialize};

use nanocl_error::io::IoResult;

use nanocld_client::stubs::system::Event;
use nanocld_client::stubs::resource::Resource;

use crate::manager::NCertManager;
use crate::utils::resource::update_resource_certs;
use crate::utils::secret::{SecretMetadata, get_expiry_time};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ProxyRuleCertManagerMetadata {
  pub cert_manager_issuer: String,
}

pub async fn handle_resource_update<'a>(
  manager: &NCertManager<'a>,
  resource: Resource,
) -> IoResult<()> {
  if resource.kind.as_str() != "ProxyRule" {
    return Ok(());
  }

  match &resource.metadata {
    Some(metadata) => {
      let metadata = serde_json::from_value::<ProxyRuleCertManagerMetadata>(
        metadata.clone(),
      );

      match metadata {
        Ok(metadata) => {
          let issuer_key = metadata.cert_manager_issuer;

          log::debug!("Resource handling {}", resource.name);

          update_resource_certs(manager, resource, &issuer_key).await
        }
        Err(_) => Ok(()),
      }
    }
    None => Ok(()),
  }
}

pub async fn handle_secret_update<'a>(
  manager: &mut NCertManager<'a>,
  secret: Secret,
) -> IoResult<()> {
  if secret.kind.as_str() != "Tls" {
    return Ok(());
  }
  match &secret.metadata {
    Some(metadata) => {
      let metadata = serde_json::from_value::<SecretMetadata>(metadata.clone());

      if metadata.is_err() {
        return Ok(());
      }

      let expiry = get_expiry_time(&secret)?;

      log::debug!("Secret handling {} expire at {}", secret.key, expiry);

      manager.add_secret(secret.key, expiry);

      Ok(())
    }
    None => Ok(()),
  }
}

pub async fn handle_secret_delete<'a>(
  manager: &mut NCertManager<'a>,
  secret: Secret,
) -> IoResult<()> {
  if secret.kind.as_str() != "Tls" {
    return Ok(());
  }
  match &secret.metadata {
    Some(metadata) => {
      let metadata = serde_json::from_value::<SecretMetadata>(metadata.clone());

      if metadata.is_err() {
        return Ok(());
      }

      log::debug!("Delete secret handling {}", secret.key);

      manager.remove_secret(&secret.key);

      Ok(())
    }
    None => Ok(()),
  }
}

pub async fn handle_event<'a>(
  manager: &mut NCertManager<'a>,
  event: Option<core::result::Result<Event, HttpError>>,
) -> bool {
  match event {
    Some(event) => match event {
      Err(err) => {
        log::error!("Event stream error: {err}");
        true
      }
      Ok(event) => {
        log::debug!("Handle event");

        let result = match event {
          Event::ResourceCreated(ev) => {
            handle_resource_update(manager, *ev).await
          }
          Event::ResourcePatched(ev) => {
            handle_resource_update(manager, *ev).await
          }
          Event::SecretCreated(ev) => handle_secret_update(manager, *ev).await,
          Event::SecretPatched(ev) => handle_secret_update(manager, *ev).await,
          Event::SecretDeleted(ev) => handle_secret_delete(manager, *ev).await,
          _ => Ok(()),
        };

        if let Err(err) = result {
          log::warn!("Handle event error : {err}");
        }
        manager.debug();

        false
      }
    },
    None => {
      log::error!("Event stream end");
      true
    }
  }
}
