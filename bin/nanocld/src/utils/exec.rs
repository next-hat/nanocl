use bollard_next::service::ExecInspectResponse;
use ntex::web;

use bollard_next::container::LogOutput;
use bollard_next::exec::{StartExecOptions, StartExecResults, CreateExecResults};

use nanocl_error::http::HttpResult;
use nanocl_stubs::cargo::CreateExecOptions;
use nanocl_stubs::process::OutputLog;

use crate::models::SystemState;

use super::stream::transform_stream;

/// Create an exec command in a cargo instance and return command id
pub(crate) async fn create_exec_command(
  name: &str,
  args: &CreateExecOptions,
  state: &SystemState,
) -> HttpResult<CreateExecResults> {
  let name = format!("{name}.c");
  let result = state.docker_api.create_exec(&name, args.to_owned()).await?;
  Ok(result)
}

/// Run an exec command in a cargo instance and return the output stream
pub(crate) async fn start_exec_command(
  exec_id: &str,
  args: &StartExecOptions,
  state: &SystemState,
) -> HttpResult<web::HttpResponse> {
  let res = state
    .docker_api
    .start_exec(exec_id, Some(args.to_owned()))
    .await?;
  match res {
    StartExecResults::Detached => Ok(web::HttpResponse::Ok().finish()),
    StartExecResults::Attached { output, .. } => {
      let stream = transform_stream::<LogOutput, OutputLog>(output);
      Ok(
        web::HttpResponse::Ok()
          .content_type("nanocl/streaming-v1")
          .streaming(stream),
      )
    }
  }
}

/// Inspect a command runned in a cargo instance and return the exec infos
pub(crate) async fn inspect_exec_command(
  exec_id: &str,
  state: &SystemState,
) -> HttpResult<ExecInspectResponse> {
  let result = state.docker_api.inspect_exec(exec_id).await?;
  Ok(result)
}
