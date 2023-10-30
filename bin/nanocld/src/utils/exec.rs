use ntex::web;

use bollard_next::container::LogOutput;
use bollard_next::exec::{StartExecOptions, StartExecResults};

use nanocl_error::http::HttpError;
use nanocl_stubs::cargo::{OutputLog, CreateExecOptions};

use crate::models::DaemonState;

use super::stream::transform_stream;

/// ## Create exec command
///
/// Create an exec command in a cargo instance and return command id
///
/// ## Arguments
///
/// * [name](str) - The cargo name
/// * [args](CreateExecOptions) - The exec options
/// * [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///  * [Ok](bollard_next::exec::CreateExecResults) - The output stream
///  * [Err](HttpError) - The command has not been executed
///
pub async fn create_exec_command(
  name: &str,
  args: &CreateExecOptions,
  state: &DaemonState,
) -> Result<bollard_next::exec::CreateExecResults, HttpError> {
  let name = format!("{name}.c");
  let result = state.docker_api.create_exec(&name, args.to_owned()).await?;

  Ok(result)
}

/// ## Start exec command
///
/// Run an exec command in a cargo instance and return the output stream
///
/// ## Arguments
///
/// * [name](str) - The cargo name
/// * [exec_id](str) - The cargo name
/// * [args](StartExecOptions) - The exec options
/// * [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///  * [Ok](web::HttpResponse) - The output stream
///  * [Err](HttpError) - The command has not been executed
///
pub async fn start_exec_command(
  exec_id: &str,
  args: &StartExecOptions,
  state: &DaemonState,
) -> Result<web::HttpResponse, HttpError> {
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

/// ## Exec inspect command
///
/// Inspect a command runned in a cargo instance and return the exec infos
///
/// ## Arguments
///
/// * [exec_id](String) - Exec command id to inspect
/// * [state](DaemonState) - The daemon state
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///  * [Ok](bollard_next::models::ExecInspectResponse) - The output stream
///  * [Err](HttpError) - The command has not been found
///
pub async fn inspect_exec_command(
  exec_id: &str,
  state: &DaemonState,
) -> Result<bollard_next::models::ExecInspectResponse, HttpError> {
  let result = state.docker_api.inspect_exec(exec_id).await?;

  Ok(result)
}
