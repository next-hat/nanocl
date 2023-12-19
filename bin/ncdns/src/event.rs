use ntex::rt;
use ntex::http;

use nanocl_error::io::IoResult;
use nanocl_error::http_client::HttpClientError;
use nanocl_utils::versioning;

use nanocld_client::NanocldClient;
use nanocld_client::stubs::resource::ResourcePartial;

use crate::version;

async fn ensure_self_config(client: &NanocldClient) -> IoResult<()> {
  let formated_version = versioning::format_version(version::VERSION);
  let dns_rule_kind = ResourcePartial {
    kind: "Kind".to_owned(),
    name: "DnsRule".to_owned(),
    data: serde_json::json!({
      "Url": "unix:///run/nanocl/dns.sock"
    }),
    version: format!("v{formated_version}"),
    metadata: None,
  };
  match client.inspect_resource(&dns_rule_kind.name).await {
    Ok(_) => {
      if let Err(err) = client
        .put_resource(&dns_rule_kind.name, &dns_rule_kind.clone().into())
        .await
      {
        match err {
          HttpClientError::HttpError(err)
            if err.status == http::StatusCode::CONFLICT =>
          {
            log::info!("event::ensure_self_config: up to date");
            return Ok(());
          }
          _ => {
            log::warn!("event::ensure_self_config: {err}");
            return Err(err.into());
          }
        }
      }
      Ok(())
    }
    Err(_) => {
      if let Err(err) = client.create_resource(&dns_rule_kind).await {
        match err {
          HttpClientError::HttpError(err)
            if err.status == http::StatusCode::CONFLICT =>
          {
            log::info!("event::ensure_self_config: up to date");
            return Ok(());
          }
          _ => {
            log::warn!("event::ensure_self_config: {err}");
            return Err(err.into());
          }
        }
      }
      Ok(())
    }
  }
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
