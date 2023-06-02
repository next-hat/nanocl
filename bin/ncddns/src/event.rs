use ntex::rt;
use futures::StreamExt;
use nanocl_utils::versioning;
use nanocl_utils::http_client_error::HttpClientError;
use nanocld_client::NanocldClient;
use nanocld_client::stubs::resource::ResourcePartial;

use crate::version;

async fn ensure_resource_config(client: &NanocldClient) {
  let formated_version = versioning::format_version(version::VERSION);
  let dns_rule_kind = ResourcePartial {
    kind: "Kind".to_string(),
    name: "DnsRule".to_string(),
    config: serde_json::json!({
      "Url": "unix:///run/nanocl/dns.sock"
    }),
    version: format!("v{formated_version}"),
  };

  if let Err(err) = client.create_resource(&dns_rule_kind).await {
    match err {
      HttpClientError::HttpError(err) if err.status == 409 => {
        log::info!("DnsRule already exists. Skipping.")
      }
      _ => {
        log::warn!("Unable to create DnsRule: {err}");
      }
    }
  }
}

async fn r#loop(client: &NanocldClient) {
  loop {
    log::info!("Subscribing to nanocl daemon events..");
    match client.watch_events().await {
      Err(err) => {
        log::warn!("Unable to Subscribe to nanocl daemon events: {err}");
      }
      Ok(mut stream) => {
        log::info!("Subscribed to nanocl daemon events");

        ensure_resource_config(client).await;

        while let Some(event) = stream.next().await {
          let Ok(_) = event else {
            break;
          };
        }
      }
    }
    log::warn!(
      "Unsubscribed from nanocl daemon events, retrying to subscribe in 2 seconds"
    );
    ntex::time::sleep(std::time::Duration::from_secs(2)).await;
  }
}

/// Spawn new thread with event loop to watch for nanocld events
pub(crate) fn spawn() {
  rt::Arbiter::new().exec_fn(move || {
    let client = NanocldClient::connect_with_unix_default();
    ntex::rt::spawn(async move {
      r#loop(&client).await;
    });
  });
}
