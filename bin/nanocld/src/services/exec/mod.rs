use ntex::web;

pub mod create;
pub mod inspect;
pub mod start;

pub use create::*;
pub use inspect::*;
pub use start::*;

pub fn ntex_config(config: &mut web::ServiceConfig) {
  config.service(create_exec_command);
  config.service(start_exec_command);
  config.service(inspect_exec_command);
}

#[cfg(test)]
mod tests {

  use bollard_next::exec::{
    CreateExecOptions, CreateExecResults, StartExecOptions,
  };
  use bollard_next::service::ExecInspectResponse;
  use futures::{StreamExt, TryStreamExt};
  use ntex::http;

  use nanocl_stubs::generic::GenericNspQuery;

  use crate::utils::tests::*;

  #[ntex::test]
  async fn exec() {
    const CARGO_NAME: &str = "nstore";
    let system = gen_default_test_system().await;
    let client = system.client;
    let mut res = client
      .send_post(
        &format!("/cargoes/{CARGO_NAME}/exec"),
        Some(&CreateExecOptions {
          cmd: Some(vec!["ls".into(), "/".into(), "-lra".into()]),
          attach_stderr: Some(true),
          attach_stdout: Some(true),
          ..Default::default()
        }),
        Some(&GenericNspQuery {
          namespace: Some("system".into()),
        }),
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "cargo create exec");
    let data = res.json::<CreateExecResults>().await.unwrap();
    let exec_id = data.id;
    let res = client
      .send_post(
        &format!("/exec/{exec_id}/cargo/start"),
        Some(&StartExecOptions::default()),
        None::<String>,
      )
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "exec start");
    let mut stream = res.into_stream();
    while (stream.next().await).is_some() {}
    let mut res = client
      .send_get(&format!("/exec/{exec_id}/cargo/inspect"), None::<String>)
      .await;
    test_status_code!(res.status(), http::StatusCode::OK, "exec inspect");
    let data = res.json::<ExecInspectResponse>().await.unwrap();
    assert_eq!(data.exit_code, Some(0), "Expect exit code to be 0");
  }
}
