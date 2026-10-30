use std::{
  collections::HashMap,
  sync::{Arc, Mutex, OnceLock},
};

use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::{
  resource::{Resource, ResourcePartial, ResourceUpdate},
  resource_kind::{ResourceKindPartial, ResourceKindSpec},
};
use ntex::{http::StatusCode, web};
use tokio::sync::oneshot;

use super::*;
use crate::{
  models::{ResourceKindDb, SpecDb},
  utils::tests::gen_default_test_system,
};

// Scoped one-shot barriers affect only the fixture's unique Resource name.
enum PublicationFault {
  Pause(oneshot::Sender<()>, oneshot::Receiver<()>),
  Fail,
}

fn faults() -> &'static Mutex<HashMap<String, PublicationFault>> {
  static FAULTS: OnceLock<Mutex<HashMap<String, PublicationFault>>> =
    OnceLock::new();
  FAULTS.get_or_init(Default::default)
}

pub(super) async fn before_publication(key: &str) -> IoResult<()> {
  let fault = faults().lock().unwrap().remove(key);
  match fault {
    Some(PublicationFault::Pause(entered, release)) => {
      let _ = entered.send(());
      let _ = release.await;
      Ok(())
    }
    Some(PublicationFault::Fail) => {
      Err(IoError::other("Resource fixture", "publication rejected"))
    }
    None => Ok(()),
  }
}

fn pause(key: &str) -> (oneshot::Receiver<()>, oneshot::Sender<()>) {
  let (entered, ready) = oneshot::channel();
  let (release, held) = oneshot::channel();
  faults()
    .lock()
    .unwrap()
    .insert(key.to_owned(), PublicationFault::Pause(entered, held));
  (ready, release)
}

#[ntex::test]
async fn resource_mutation_owns_ordering_after_waiter_cancellation() {
  let key = uuid::Uuid::new_v4().to_string();
  let log = Arc::new(Mutex::new(Vec::new()));
  let (entered, ready) = oneshot::channel();
  let (release, held) = oneshot::channel();
  let first_log = log.clone();
  let mut first = Box::pin(ResourceDb::mutate(&key, async move {
    first_log.lock().unwrap().push("apply A");
    entered.send(()).unwrap();
    held.await.unwrap();
    first_log.lock().unwrap().extend(["publish A", "event A"]);
    Ok(())
  }));
  assert!(futures::poll!(first.as_mut()).is_pending());
  ready.await.unwrap();
  drop(first);
  let second_log = log.clone();
  let mut second = Box::pin(ResourceDb::mutate(&key, async move {
    second_log
      .lock()
      .unwrap()
      .extend(["apply B", "publish B", "event B"]);
    Ok(())
  }));
  assert!(futures::poll!(second.as_mut()).is_pending());
  ResourceDb::mutate("independent-resource", async { Ok(()) })
    .await
    .unwrap();
  assert_eq!(*log.lock().unwrap(), vec!["apply A"]);
  release.send(()).unwrap();
  second.await.unwrap();
  assert_eq!(
    *log.lock().unwrap(),
    vec![
      "apply A",
      "publish A",
      "event A",
      "apply B",
      "publish B",
      "event B"
    ]
  );
}

#[ntex::test]
async fn resource_mutation_failure_releases_same_name() {
  let key = uuid::Uuid::new_v4().to_string();
  assert!(
    ResourceDb::mutate::<()>(&key, async {
      Err(HttpError::bad_gateway("controller unavailable"))
    })
    .await
    .is_err()
  );
  ResourceDb::mutate(&key, async { Ok(()) }).await.unwrap();
}

#[derive(Default)]
struct Controller {
  rules: HashMap<String, serde_json::Value>,
  applies: usize,
  delete_status: Option<StatusCode>,
}

// Uses the normal daemon database fixture and a dummy HTTP controller. Compile
// locally; run only in CI/the owner-provided integration environment.
#[ntex::test]
async fn resource_publication_orders_updates_and_duplicate_creates() {
  let system = gen_default_test_system().await;
  let controller = Arc::new(Mutex::new(Controller::default()));
  let observed = controller.clone();
  let server = web::test::server(async move |_| {
    let controller = controller.clone();
    web::App::new().service(web::resource("/{version}/rules/{name}").route(
      web::to(move |request: web::HttpRequest, body: ntex::util::Bytes| {
        let controller = controller.clone();
        async move {
          let key = request.match_info().get("name").unwrap().to_owned();
          let mut controller = controller.lock().unwrap();
          if request.method() == ntex::http::Method::DELETE {
            if let Some(status) = controller.delete_status {
              return web::HttpResponse::builder(status)
                .json(&serde_json::json!({"msg":"delete rejected"}));
            }
            controller.rules.remove(&key);
            web::HttpResponse::Ok().build()
          } else {
            let value: serde_json::Value =
              serde_json::from_slice(&body).unwrap();
            controller.applies += 1;
            controller.rules.insert(key, value.clone());
            web::HttpResponse::Ok().json(&value)
          }
        }
      }),
    ))
  });
  let pool = &system.state.inner.pool;
  let kind = format!("test.io/resource-{}", uuid::Uuid::new_v4());
  ResourceKindDb::create_from_spec(
    &ResourceKindPartial {
      name: kind.clone(),
      version: "v1".into(),
      metadata: None,
      data: ResourceKindSpec {
        schema: None,
        url: Some(server.url("").trim_end_matches('/').to_owned()),
      },
    },
    pool,
  )
  .await
  .unwrap();
  let name = format!("resource-{}", uuid::Uuid::new_v4());
  let spec = ResourcePartial {
    name: name.clone(),
    kind: kind.clone(),
    data: serde_json::json!({"revision":"initial"}),
    metadata: None,
  };
  let response = system
    .client
    .send_post("/resources", Some(&spec), None::<String>)
    .await;
  assert_eq!(response.status(), StatusCode::CREATED);
  let initial = response.json::<Resource>().await.unwrap();

  let endpoint = format!("/resources/{name}");
  let update_a = ResourceUpdate {
    data: serde_json::json!({"revision":"A"}),
    metadata: None,
  };
  let update_b = ResourceUpdate {
    data: serde_json::json!({"revision":"B"}),
    metadata: None,
  };
  let (ready, release) = pause(&name);
  let mut first = Box::pin(system.client.send_put(
    &endpoint,
    Some(&update_a),
    None::<String>,
  ));
  assert!(futures::poll!(first.as_mut()).is_pending());
  ready.await.unwrap();
  let mut second = Box::pin(system.client.send_put(
    &endpoint,
    Some(&update_b),
    None::<String>,
  ));
  assert!(
    ntex::time::timeout(std::time::Duration::from_millis(100), second.as_mut())
      .await
      .is_err()
  );
  assert_eq!(observed.lock().unwrap().rules[&name], update_a.data);
  assert_eq!(
    ResourceDb::transform_read_by_pk(&name, pool)
      .await
      .unwrap()
      .spec
      .key,
    initial.spec.key
  );
  release.send(()).unwrap();
  assert_eq!(first.await.status(), StatusCode::OK);
  assert_eq!(second.await.status(), StatusCode::OK);
  assert_eq!(observed.lock().unwrap().rules[&name], update_b.data);
  let accepted = ResourceDb::transform_read_by_pk(&name, pool).await.unwrap();
  assert_eq!(accepted.spec.data, update_b.data);
  assert_eq!(
    ResourceDb::history(&name, None, None, pool)
      .await
      .unwrap()
      .len(),
    3
  );

  // A rejected publication must restore the canonical controller revision.
  faults()
    .lock()
    .unwrap()
    .insert(name.clone(), PublicationFault::Fail);
  let response = system
    .client
    .send_put(&endpoint, Some(&update_a), None::<String>)
    .await;
  assert!(response.status().is_server_error());
  assert_eq!(observed.lock().unwrap().rules[&name], update_b.data);
  assert_eq!(
    ResourceDb::transform_read_by_pk(&name, pool)
      .await
      .unwrap()
      .spec
      .key,
    accepted.spec.key
  );
  assert_eq!(
    ResourceDb::history(&name, None, None, pool)
      .await
      .unwrap()
      .len(),
    3
  );

  // A failed controller deletion retains both declaration and history; the
  // exact same normal DELETE succeeds once the controller returns.
  for status in [
    StatusCode::BAD_REQUEST,
    StatusCode::UNAUTHORIZED,
    StatusCode::FORBIDDEN,
    StatusCode::CONFLICT,
    StatusCode::SERVICE_UNAVAILABLE,
  ] {
    observed.lock().unwrap().delete_status = Some(status);
    let response = system.client.send_delete(&endpoint, None::<String>).await;
    assert_eq!(response.status(), status);
    assert_eq!(
      ResourceDb::transform_read_by_pk(&name, pool)
        .await
        .unwrap()
        .spec
        .key,
      accepted.spec.key,
    );
    assert_eq!(
      ResourceDb::history(&name, None, None, pool)
        .await
        .unwrap()
        .len(),
      3
    );
    assert!(observed.lock().unwrap().rules.contains_key(&name));
  }
  observed.lock().unwrap().delete_status = None;
  assert_eq!(
    system
      .client
      .send_delete(&endpoint, None::<String>)
      .await
      .status(),
    StatusCode::ACCEPTED
  );
  assert!(ResourceDb::transform_read_by_pk(&name, pool).await.is_err());
  assert!(
    ResourceDb::history(&name, None, None, pool)
      .await
      .unwrap()
      .is_empty()
  );
  assert!(!observed.lock().unwrap().rules.contains_key(&name));

  // Both requests use the same name; only the winner may reach the controller.
  let count = observed.lock().unwrap().applies;
  let (ready, release) = pause(&name);
  let mut first = Box::pin(system.client.send_post(
    "/resources",
    Some(&spec),
    None::<String>,
  ));
  assert!(futures::poll!(first.as_mut()).is_pending());
  ready.await.unwrap();
  let mut loser_spec = spec.clone();
  loser_spec.data = update_b.data;
  let mut second = Box::pin(system.client.send_post(
    "/resources",
    Some(&loser_spec),
    None::<String>,
  ));
  assert!(
    ntex::time::timeout(std::time::Duration::from_millis(100), second.as_mut())
      .await
      .is_err()
  );
  release.send(()).unwrap();
  assert_eq!(first.await.status(), StatusCode::CREATED);
  assert_eq!(second.await.status(), StatusCode::CONFLICT);
  assert_eq!(observed.lock().unwrap().applies, count + 1);
  assert_eq!(observed.lock().unwrap().rules[&name], spec.data);
  assert_eq!(
    ResourceDb::history(&name, None, None, pool)
      .await
      .unwrap()
      .len(),
    1
  );

  // The spec insert and resource insert must roll back together on conflict.
  assert!(
    ResourceDb::create_from_spec(&loser_spec, pool)
      .await
      .is_err()
  );
  assert_eq!(
    ResourceDb::history(&name, None, None, pool)
      .await
      .unwrap()
      .len(),
    1
  );
  observed.lock().unwrap().rules.remove(&name);
  observed.lock().unwrap().delete_status = Some(StatusCode::NOT_FOUND);
  let stored = ResourceDb::transform_read_by_pk(&name, pool).await.unwrap();
  assert_eq!(
    ResourceDb::hook_delete(&stored, pool)
      .await
      .unwrap_err()
      .status,
    StatusCode::NOT_FOUND
  );
  assert_eq!(
    system
      .client
      .send_delete(&endpoint, None::<String>)
      .await
      .status(),
    StatusCode::ACCEPTED
  );

  assert!(ResourceDb::transform_read_by_pk(&name, pool).await.is_err());
  assert!(
    ResourceDb::history(&name, None, None, pool)
      .await
      .unwrap()
      .is_empty()
  );
  observed.lock().unwrap().delete_status = None;

  // A failed first publication removes its candidate controller configuration.
  faults()
    .lock()
    .unwrap()
    .insert(name.clone(), PublicationFault::Fail);
  let response = system
    .client
    .send_post("/resources", Some(&spec), None::<String>)
    .await;
  assert!(response.status().is_server_error());
  assert!(!observed.lock().unwrap().rules.contains_key(&name));
  assert!(
    ResourceDb::history(&name, None, None, pool)
      .await
      .unwrap()
      .is_empty()
  );
  ResourceKindDb::del_by_pk(&kind, pool).await.unwrap();
  SpecDb::del_by_kind_key("ResourceKind", &kind, pool)
    .await
    .unwrap();
}

// Database regression: an orphaned registration must not make its Resource
// permanently undeletable through the normal API.
#[ntex::test]
async fn resource_delete_removes_orphaned_kind_and_version() {
  let system = gen_default_test_system().await;
  let pool = &system.state.inner.pool;
  let kind = format!("test.io/orphan-{}", uuid::Uuid::new_v4());
  ResourceKindDb::create_from_spec(
    &ResourceKindPartial {
      name: kind.clone(),
      version: "v1".into(),
      metadata: None,
      data: ResourceKindSpec {
        schema: Some(serde_json::json!({})),
        url: None,
      },
    },
    pool,
  )
  .await
  .unwrap();
  let name = format!("orphan-{}", uuid::Uuid::new_v4());
  let resource = ResourceDb::create_from_spec(
    &ResourcePartial {
      name: name.clone(),
      kind: kind.clone(),
      data: serde_json::json!({}),
      metadata: None,
    },
    pool,
  )
  .await
  .unwrap();

  // Explicit version lookup: absence permits normal deletion, but does not
  // provide proof that an uncommitted controller candidate was removed.
  let mut missing_version = resource.clone();
  missing_version.kind = format!("{kind}/missing");
  assert!(
    ResourceDb::hook_delete_existing(&missing_version, pool)
      .await
      .is_ok()
  );
  assert_eq!(
    ResourceDb::hook_delete(&missing_version, pool)
      .await
      .unwrap_err()
      .status,
    StatusCode::NOT_FOUND
  );

  ResourceKindDb::del_by_pk(&kind, pool).await.unwrap();
  SpecDb::del_by_kind_key("ResourceKind", &kind, pool)
    .await
    .unwrap();
  assert_eq!(
    ResourceDb::hook_delete(&resource, pool)
      .await
      .unwrap_err()
      .status,
    StatusCode::NOT_FOUND
  );
  assert_eq!(
    system
      .client
      .send_delete(&format!("/resources/{name}"), None::<String>)
      .await
      .status(),
    StatusCode::ACCEPTED
  );
  assert!(ResourceDb::transform_read_by_pk(&name, pool).await.is_err());
  assert!(
    ResourceDb::history(&name, None, None, pool)
      .await
      .unwrap()
      .is_empty()
  );
}
