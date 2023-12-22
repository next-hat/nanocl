use ntex::rt;

use nanocl_error::io::IoResult;

use nanocl_utils::versioning;

use nanocld_client::stubs::resource_kind::{ResourceKindPartial, ResourceKindSpec};

use nanocld_client::NanocldClient;

use crate::version;

async fn ensure_self_config(client: &NanocldClient) -> IoResult<()> {
  let formated_version = versioning::format_version(version::VERSION);
  let resource_kind = ResourceKindPartial {
    name: "ncdns.io/rule".to_owned(),
    version: format!("v{formated_version}"),
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

async fn r#loop(client: &NanocldClient) {
  loop {
    log::info!("event::loop: subscribing to nanocld events");
    match client.watch_events().await {
      Err(err) => {
        log::warn!("event::loop: {err}");
      }
      Ok(_) => {
        log::info!("event::loop: subscribed to nanocld events");
        if ensure_self_config(client).await.is_ok() {
          break;
        }
      }
    }
    log::warn!("event::loop: retrying in 2 seconds");
    ntex::time::sleep(std::time::Duration::from_secs(2)).await;
  }
}

/// Spawn new thread with event loop to watch for nanocld events
pub(crate) fn spawn(client: &NanocldClient) {
  let client = client.clone();
  rt::Arbiter::new().exec_fn(move || {
    ntex::rt::spawn(async move {
      r#loop(&client).await;
      rt::Arbiter::current().stop();
    });
  });
}
