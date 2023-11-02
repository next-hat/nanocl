use ntex::rt;
use ntex::http;
use futures::StreamExt;

use nanocl_error::http_client::HttpClientError;
use nanocl_utils::versioning;
use nanocld_client::NanocldClient;
use nanocld_client::stubs::system::Event;
use nanocld_client::stubs::dns::ResourceDnsRule;
use nanocld_client::stubs::resource::ResourcePartial;

use crate::{utils, version, dnsmasq};

async fn ensure_resource_config(client: &NanocldClient) {
  let formated_version = versioning::format_version(version::VERSION);
  let dns_rule_kind = ResourcePartial {
    kind: "Kind".to_string(),
    name: "DnsRule".to_string(),
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
            log::info!("DnsRule already exists. Skipping.")
          }
          _ => {
            log::warn!("Unable to update DnsRule: {err}");
          }
        }
      }
    }
    Err(_) => {
      if let Err(err) = client.create_resource(&dns_rule_kind).await {
        match err {
          HttpClientError::HttpError(err)
            if err.status == http::StatusCode::CONFLICT =>
          {
            log::info!("DnsRule already exists. Skipping.")
          }
          _ => {
            log::warn!("Unable to create DnsRule: {err}");
          }
        }
      }
    }
  }
}

async fn r#loop(dnsmasq: &dnsmasq::Dnsmasq, client: &NanocldClient) {
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
          let Ok(e) = event else {
            break;
          };
          match e {
            Event::ResourceCreated(resource) => {
              let dns_rule =
                serde_json::from_value::<ResourceDnsRule>(resource.data);
              let Ok(dns_rule) = dns_rule else {
                log::warn!("Unable to serialize the DnsRule");
                continue;
              };
              if let Err(err) =
                utils::update_entries(&dns_rule, dnsmasq, client).await
              {
                log::error!("Unable to update the DnsRule: {err}");
              }
            }
            Event::ResourcePatched(resource) => {
              let dns_rule =
                serde_json::from_value::<ResourceDnsRule>(resource.data);
              let Ok(dns_rule) = dns_rule else {
                log::warn!("Unable to serialize the DnsRule");
                continue;
              };
              if let Err(err) =
                utils::update_entries(&dns_rule, dnsmasq, client).await
              {
                log::error!("Unable to update the DnsRule: {err}");
              }
            }
            Event::ResourceDeleted(resource) => {
              log::info!("Resource deleted: {resource:#?}");
            }
            _ => {
              log::info!("Ignoring event: {e}");
            }
          }
        }
      }
    }
    log::warn!("Retrying to subscribe in 2 seconds");
    ntex::time::sleep(std::time::Duration::from_secs(2)).await;
  }
}

/// Spawn new thread with event loop to watch for nanocld events
pub(crate) fn spawn(dnsmasq: &dnsmasq::Dnsmasq, client: &NanocldClient) {
  let dnsmasq = dnsmasq.clone();
  let client = client.clone();
  rt::Arbiter::new().exec_fn(move || {
    ntex::rt::spawn(async move {
      r#loop(&dnsmasq, &client).await;
    });
  });
}
