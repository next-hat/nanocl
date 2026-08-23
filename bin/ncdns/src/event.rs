use std::time::Duration;

use ntex::rt;

use nanocl_error::io::IoResult;
use nanocl_utils::versioning;
use nanocld_client::NanocldClient;
use nanocld_client::stubs::resource_kind::{
  ResourceKindPartial, ResourceKindSpec,
};

use crate::vars;

const RETRY_DELAY: Duration = Duration::from_secs(2);

pub(super) async fn ensure_self_config(client: &NanocldClient) -> IoResult<()> {
  let formatted_version = versioning::format_version(vars::VERSION);
  let resource_kind = ResourceKindPartial {
    name: vars::RULE_KEY.to_owned(),
    version: format!("v{formatted_version}"),
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

async fn ensure_loop(client: &NanocldClient) {
  loop {
    match ensure_self_config(client).await {
      Ok(()) => return,
      Err(err) => {
        log::warn!("event::loop: unable to ensure resource kind: {err}");
        ntex::time::sleep(RETRY_DELAY).await;
      }
    }
  }
}

/// Spawn a background task to register the ncdns ResourceKind.
pub(crate) fn spawn(client: &NanocldClient) {
  let client = client.clone();
  rt::Arbiter::new().handle().spawn(async move {
    rt::spawn(async move {
      ensure_loop(&client).await;
      rt::Arbiter::current().stop();
    });
  });
}
