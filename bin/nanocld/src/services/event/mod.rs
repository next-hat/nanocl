use ntex::web;

mod count;
mod inspect;
mod list;
mod watch;

pub use count::*;
pub use inspect::*;
pub use list::*;
pub use watch::*;

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(list_event);
  config.service(watch_event);
  config.service(inspect_event);
  config.service(count_event);
}

#[cfg(test)]
mod tests {
  use bollard_next::container::Config;
  use futures::{StreamExt, TryStreamExt};
  use nanocl_stubs::{
    cargo_spec::CargoSpecPartial,
    system::{
      Event, EventActorKind, EventCondition, EventKind, NativeEventAction,
    },
  };
  use ntex::{http, rt};

  use crate::utils::tests::*;

  #[ntex::test]
  async fn basic() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let mut resp = client.get("/events").send().await.unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
    let events = resp.json::<Vec<Event>>().await.unwrap();
    assert!(!events.is_empty());
    let mut resp = client
      .get(&format!("/events/{}/inspect", events[0].key))
      .send()
      .await
      .unwrap();
    assert_eq!(resp.status(), http::StatusCode::OK);
    resp.json::<Event>().await.unwrap();
  }

  #[ntex::test]
  async fn watch_events() {
    let system = gen_default_test_system().await;
    let client = system.client;
    let res = client
      .send_post("/events/watch", None::<String>, None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "watch events");
  }

  #[ntex::test]
  async fn watch_events_condition() {
    const CARGO_NAME: &str = "event-condition";
    let system = gen_default_test_system().await;
    let client = system.client;
    let client_ptr = client.clone();
    let conditions = [EventCondition {
      actor_kind: Some(EventActorKind::Cargo),
      actor_key: Some(format!("{CARGO_NAME}.global")),
      kind: [EventKind::Normal].to_vec(),
      action: [NativeEventAction::Start].to_vec(),
      ..Default::default()
    }];
    let wait_task = rt::spawn(async move {
      let res = client_ptr
        .send_post("/events/watch", Some(conditions), None::<String>)
        .await;
      test_status_code!(
        res.status(),
        http::StatusCode::OK,
        "watch events condition"
      );
      let mut stream = res.into_stream();
      while (stream.next().await).is_some() {}
    });
    let cargo = CargoSpecPartial {
      name: CARGO_NAME.to_owned(),
      container: Config {
        image: Some("alpine:latest".to_owned()),
        ..Default::default()
      },
      ..Default::default()
    };
    let _ = client
      .send_post("/cargoes", Some(cargo), None::<String>)
      .await;
    let _ = client
      .send_post(
        &format!("/processes/cargo/{CARGO_NAME}/start"),
        None::<String>,
        None::<String>,
      )
      .await;
    assert!(wait_task.await.is_ok());
    let _ = client
      .send_delete(&format!("/cargoes/{CARGO_NAME}"), None::<String>)
      .await;
  }
}
