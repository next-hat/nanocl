pub mod nginx;
pub mod resource;
pub mod rule;
pub mod server;

#[cfg(test)]
pub(crate) mod tests {
  use std::{
    path::PathBuf,
    sync::{
      Arc,
      atomic::{AtomicU64, Ordering},
    },
    time::Duration,
  };

  use futures::StreamExt;

  use bollard_next::models::ContainerCreateBody as Config;
  use nanocl_error::io::{FromIo, IoError, IoResult};
  pub use nanocl_utils::ntex::test_client::*;

  use nanocld_client::{
    ConnectOpts, NanocldClient,
    stubs::{
      cargo::CargoDeleteQuery,
      cargo_spec::{CargoSpec, ContainerSpec},
      generic::ImagePullPolicy,
      proxy::ResourceProxyRule,
      system::{EventActorKind, EventKind, NativeEventAction},
    },
  };

  use crate::{
    models::{EventEmitter, Store, SystemEventKind, SystemState},
    services, vars,
  };

  static NEXT_TEST_DIR: AtomicU64 = AtomicU64::new(0);

  pub struct TestConfig {
    pub state_dir: PathBuf,
    // Keep handler reload notifications local without starting a supervisor.
    _reload_events: futures::channel::mpsc::UnboundedReceiver<SystemEventKind>,
  }

  impl Drop for TestConfig {
    fn drop(&mut self) {
      let _ = std::fs::remove_dir_all(&self.state_dir);
    }
  }

  pub fn before() {
    // Set test env to true to setup test logger
    unsafe {
      std::env::set_var("TEST", "true");
    }
  }

  pub async fn ensure_test_cargo() -> IoResult<()> {
    const CARGO_NAME: &str = "ncproxy-test";
    const CARGO_KEY: &str = "global.ncproxy-test";
    const CARGO_IMAGE: &str = "ghcr.io/next-hat/nanocl-get-started:latest";
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".to_owned(),
      ..Default::default()
    })?;
    if client.inspect_cargo(CARGO_KEY).await.is_err() {
      let cargo = CargoSpec {
        name: CARGO_NAME.to_owned(),
        containers: vec![ContainerSpec {
          name: "main".to_owned(),
          essential: true,
          secrets: Vec::new(),
          image_pull_secret: None,
          image_pull_policy: ImagePullPolicy::IfNotPresent,
          container_config: Config {
            image: Some(CARGO_IMAGE.to_owned()),
            ..Default::default()
          },
        }],
        ..Default::default()
      };
      client.create_cargo(&cargo, None).await?;
    }
    complete_test_cargo_action(&client, CARGO_KEY, NativeEventAction::Start)
      .await
  }

  pub async fn clean_test_cargo() -> IoResult<()> {
    const CARGO_KEY: &str = "global.ncproxy-test";
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: "http://nanocl.internal:8585".into(),
      ..Default::default()
    })?;
    if client.inspect_cargo(CARGO_KEY).await.is_err() {
      return Ok(());
    }
    complete_test_cargo_action(&client, CARGO_KEY, NativeEventAction::Destroy)
      .await
  }

  // Accepted lifecycle requests still hold their lease. Subscribe first and
  // wait for the exact operation before another test reuses the shared Cargo.
  async fn complete_test_cargo_action(
    client: &NanocldClient,
    key: &str,
    action: NativeEventAction,
  ) -> IoResult<()> {
    ntex::time::timeout(Duration::from_secs(360), async {
      // Name-only conditions can close on a preceding operation's event.
      let mut events = client.watch_events(None).await?;
      let operation = match action {
        NativeEventAction::Start => {
          client.start_process_operation("cargo", key).await?
        }
        NativeEventAction::Destroy => Some(
          client
            .delete_process_operation(
              "cargo",
              key,
              Some(CargoDeleteQuery { force: Some(true) }),
            )
            .await?,
        ),
        _ => unreachable!("only test Cargo start and deletion are supported"),
      };
      let Some(operation) = operation else {
        // An authoritative unchanged start has no completion event.
        return Ok(());
      };
      let operation = operation.to_string();
      while let Some(event) = events.next().await {
        let event = event?;
        if !event.actor.as_ref().is_some_and(|actor| {
          actor.key.as_deref() == Some(key)
            && actor.kind == EventActorKind::Cargo
        }) || event
          .metadata
          .as_ref()
          .and_then(|metadata| metadata.get("Operation"))
          .and_then(serde_json::Value::as_str)
          != Some(operation.as_str())
        {
          continue;
        }
        if event.kind == EventKind::Error || event.reason == "operation_pending"
        {
          return Err(IoError::interrupted(
            "Test Cargo",
            event
              .note
              .as_deref()
              .unwrap_or("Cargo operation did not complete"),
          ));
        }
        if event.kind == EventKind::Normal && event.action == action.to_string()
        {
          return Ok(());
        }
      }
      Err(IoError::interrupted(
        "Test Cargo",
        "Event stream ended before Cargo completion",
      ))
    })
    .await
    .map_err(|_| {
      IoError::interrupted(
        "Test Cargo",
        "Timed out waiting for Cargo completion",
      )
    })?
  }

  pub fn read_rule(path: &str) -> IoResult<ResourceProxyRule> {
    let resource = std::fs::read_to_string(path)?;
    let rule: ResourceProxyRule =
      serde_yaml::from_str(&resource).map_err(|err| {
        err.map_err_context(|| format!("Failed to parse rule: {}", path))
      })?;
    Ok(rule)
  }

  pub async fn gen_default_test_client() -> (TestClient, TestConfig) {
    before();
    let id = NEXT_TEST_DIR.fetch_add(1, Ordering::Relaxed);
    let state_dir = std::env::temp_dir()
      .join(format!("ncproxy-http-test-{}-{id}", std::process::id()));
    std::fs::create_dir(&state_dir).unwrap();
    let (sender, reload_events) = futures::channel::mpsc::unbounded();
    let config = TestConfig {
      state_dir,
      _reload_events: reload_events,
    };
    // Direct handler rules are not persisted Resources. A daemon event loop
    // would reconcile them away, and CI's live proxy owns its own directory.
    let system_state = Arc::new(SystemState {
      client: NanocldClient::connect_to(&ConnectOpts {
        url: "http://nanocl.internal:8585".into(),
        ..Default::default()
      })
      .unwrap(),
      event_emitter: EventEmitter(Arc::new(sender)),
      store: Store::new(config.state_dir.to_str().unwrap()),
      nginx_dir: "/etc/nginx".to_owned(),
      config_lock: Arc::new(futures::lock::Mutex::new(())),
    });
    super::nginx::ensure_conf(&system_state).await.unwrap();
    // Create test server
    let srv = ntex::web::test::server(async move |_| {
      ntex::web::App::new()
        .with_config(services::app_config())
        .configure(services::ntex_config)
        .build_with(ntex::web::AppState::new(Arc::clone(&system_state)))
    });
    (TestClient::new(srv, vars::VERSION), config)
  }
}
