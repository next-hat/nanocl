use ntex::{rt, http};
use ntex::util::Bytes;
use ntex::channel::mpsc;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;

use nanocl_error::http::{HttpResult, HttpError};

use nanocl_stubs::system::Event;
use nanocl_stubs::job::JobPartial;
use nanocl_stubs::resource::ResourcePartial;
use nanocl_stubs::secret::{SecretPartial, SecretUpdate};
use nanocl_stubs::cargo_config::CargoConfigPartial;
use nanocl_stubs::vm_config::{VmConfigPartial, VmDiskConfig};
use nanocl_stubs::state::{
  StateDeployment, StateCargo, StateVirtualMachine, StateResource, StateMeta,
  StateStream, StateSecret, StateApplyQuery, StateJob,
};

use crate::{utils, repositories};
use crate::models::{StateFileData, DaemonState};

/// ## Ensure namespace existence
///
/// Ensure that the namespace exists in the system
///
/// ## Arguments
///
/// * [namespace](Option) - The optional [namespace name](String)
/// * [state](DaemonState) - The system state
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [String](String)
///
async fn ensure_namespace_existence(
  namespace: &Option<String>,
  state: &DaemonState,
) -> HttpResult<String> {
  if let Some(namespace) = namespace {
    utils::namespace::create_if_not_exists(namespace, state).await?;
    return Ok(namespace.to_owned());
  }
  Ok("global".to_owned())
}

/// ## Stream to bytes
///
/// Local utility to convert a state stream to bytes to send to the client
///
/// ## Arguments
///
/// * [state_stream](StateStream) - The state stream to convert
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [Bytes](Bytes)
///
fn stream_to_bytes(state_stream: StateStream) -> HttpResult<Bytes> {
  let bytes =
    serde_json::to_string(&state_stream).map_err(|err| HttpError {
      status: http::StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("unable to serialize state_stream_to_bytes {err}"),
    })?;
  Ok(Bytes::from(bytes + "\r\n"))
}

/// ## Send
///
/// Send a state stream to the client through the sender channel
///
/// ## Arguments
///
/// * [state_stream](StateStream) - The state stream to send
/// * [sx](mpsc::Sender) - The response sender
///
fn send(state_stream: StateStream, sx: &mpsc::Sender<HttpResult<Bytes>>) {
  let _ = sx.send(stream_to_bytes(state_stream));
}

/// ## Parse State
///
/// Parse the state payload and return the data
///
/// ## Arguments
///
/// * [data](serde_json::Value) - The state payload
///
/// ## Return
///
/// [HttpResult](HttpResult) containing a [StateFileData](StateFileData)
///
pub(crate) fn parse_state(
  data: &serde_json::Value,
) -> HttpResult<StateFileData> {
  let meta =
    serde_json::from_value::<StateMeta>(data.to_owned()).map_err(|err| {
      HttpError {
        status: http::StatusCode::BAD_REQUEST,
        msg: format!("unable to serialize payload {err}"),
      }
    })?;
  match meta.kind.as_str() {
    "Deployment" => {
      let data = serde_json::from_value::<StateDeployment>(data.to_owned())
        .map_err(|err| HttpError {
          status: http::StatusCode::BAD_REQUEST,
          msg: format!("unable to serialize payload {err}"),
        })?;
      Ok(StateFileData::Deployment(data))
    }
    "Cargo" => {
      let data = serde_json::from_value::<StateCargo>(data.to_owned())
        .map_err(|err| HttpError {
          status: http::StatusCode::BAD_REQUEST,
          msg: format!("unable to serialize payload {err}"),
        })?;
      Ok(StateFileData::Cargo(data))
    }
    "VirtualMachine" => {
      let data = serde_json::from_value::<StateVirtualMachine>(data.to_owned())
        .map_err(|err| HttpError {
          status: http::StatusCode::BAD_REQUEST,
          msg: format!("unable to serialize payload {err}"),
        })?;
      Ok(StateFileData::VirtualMachine(data))
    }
    "Resource" => {
      let data = serde_json::from_value::<StateResource>(data.to_owned())
        .map_err(|err| HttpError {
          status: http::StatusCode::BAD_REQUEST,
          msg: format!("unable to serialize payload {err}"),
        })?;
      Ok(StateFileData::Resource(data))
    }
    "Job" => {
      let data =
        serde_json::from_value::<StateJob>(data.to_owned()).map_err(|err| {
          HttpError {
            status: http::StatusCode::BAD_REQUEST,
            msg: format!("unable to serialize payload {err}"),
          }
        })?;
      Ok(StateFileData::Job(data))
    }
    "Secret" => {
      let data = serde_json::from_value::<StateSecret>(data.to_owned())
        .map_err(|err| HttpError {
          status: http::StatusCode::BAD_REQUEST,
          msg: format!("unable to serialize payload {err}"),
        })?;
      Ok(StateFileData::Secret(data))
    }
    _ => Err(HttpError {
      status: http::StatusCode::BAD_REQUEST,
      msg: format!("Unknown Statefile Kind: {}", meta.kind),
    }),
  }
}

/// ## Apply Secret
///
/// Apply the list of secrets to the system.
/// It will create the secrets if they don't exist.
/// If they exists but are not up to date, it will update them.
///
/// ## Arguments
///
/// * [data](Vec<SecretPartial>) - The list of secrets to apply
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender) - The response sender
///
async fn apply_secrets(
  data: &[SecretPartial],
  state: &DaemonState,
  qs: &StateApplyQuery,
  sx: &mpsc::Sender<HttpResult<Bytes>>,
) {
  data
    .iter()
    .map(|secret| async {
      let key = secret.key.to_owned();
      send(StateStream::new_secret_pending(&key), sx);
      match repositories::secret::find_by_key(&key, &state.pool).await {
        Ok(existing) => {
          let existing: SecretPartial = existing.clone().into();
          if existing == *secret && !qs.reload.unwrap_or(false) {
            send(StateStream::new_secret_unchanged(&key), sx);
            return;
          }
          if let Err(err) = repositories::secret::update_by_key(
            &key,
            &SecretUpdate {
              data: secret.data.to_owned(),
              metadata: secret.metadata.to_owned(),
            },
            &state.pool,
          )
          .await
          {
            send(StateStream::new_secret_error(&key, &err.to_string()), sx);
            return;
          }
        }
        Err(_err) => {
          if let Err(err) =
            repositories::secret::create(secret, &state.pool).await
          {
            send(StateStream::new_secret_error(&key, &err.to_string()), sx);
            return;
          }
        }
      };
      let key_ptr = key.clone();
      let state_ptr = state.clone();
      rt::spawn(async move {
        let secret =
          repositories::secret::find_by_key(&key_ptr, &state_ptr.pool)
            .await
            .unwrap();
        let _ = state_ptr
          .event_emitter
          .emit(Event::SecretPatched(Box::new(secret.into())))
          .await;
      });
      send(StateStream::new_secret_success(&key), sx);
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// ## Apply jobs
///
/// Apply the list of jobs to the system.
///
/// ## Arguments
///
/// * [data](Vec<JobPartial>) - The list of jobs to apply
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender) - The response sender
///
async fn apply_jobs(
  data: &[JobPartial],
  state: &DaemonState,
  qs: &StateApplyQuery,
  sx: &mpsc::Sender<HttpResult<Bytes>>,
) {
  data
    .iter()
    .map(|job| async move {
      send(StateStream::new_job_pending(&job.name), sx);
      match utils::job::inspect_by_name(&job.name, state).await {
        Ok(existing) => {
          let existing: JobPartial = existing.into();
          if existing == *job && !qs.reload.unwrap_or(false) {
            send(StateStream::new_job_unchanged(&job.name), sx);
            return;
          }
          if let Err(err) = utils::job::delete_by_name(&job.name, state).await {
            send(StateStream::new_job_error(&job.name, &err.to_string()), sx);
            return;
          }
          if let Err(err) = utils::job::create(job, state).await {
            send(StateStream::new_job_error(&job.name, &err.to_string()), sx);
            return;
          }
          if let Err(err) = utils::job::start_by_name(&job.name, state).await {
            send(StateStream::new_job_error(&job.name, &err.to_string()), sx);
            return;
          }
        }
        Err(_err) => {
          if let Err(err) = utils::job::create(job, state).await {
            send(StateStream::new_job_error(&job.name, &err.to_string()), sx);
            return;
          }
          if let Err(err) = utils::job::start_by_name(&job.name, state).await {
            send(StateStream::new_job_error(&job.name, &err.to_string()), sx);
            return;
          }
        }
      };
      send(StateStream::new_job_success(&job.name), sx);
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// ## Apply cargoes
///
/// Apply the list of cargoes to the system.
/// It will create the cargoes if they don't exist, and start them.
/// If they exists but are not up to date, it will update them.
///
/// ## Arguments
///
/// * [namespace](str) - The namespace name
/// * [data](Vec<CargoConfigPartial>) - The list of cargoes to apply
/// * [version](str) - The version of the cargoes
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender) - The response sender
///
async fn apply_cargoes(
  namespace: &str,
  data: &[CargoConfigPartial],
  version: &str,
  state: &DaemonState,
  qs: &StateApplyQuery,
  sx: &mpsc::Sender<HttpResult<Bytes>>,
) {
  data
    .iter()
    .map(|cargo| async {
      let key = utils::key::gen_key(namespace, &cargo.name);
      send(StateStream::new_cargo_pending(&key), sx);
      match utils::cargo::inspect_by_key(&key, state).await {
        Ok(existing) => {
          let existing: CargoConfigPartial = existing.into();
          if existing == *cargo && !qs.reload.unwrap_or(false) {
            send(StateStream::new_cargo_unchanged(&key), sx);
            return;
          }
          if let Err(err) = utils::cargo::put(&key, cargo, version, state).await
          {
            send(StateStream::new_cargo_error(&key, &err.to_string()), sx);
            return;
          }
        }
        Err(_err) => {
          if let Err(err) =
            utils::cargo::create(namespace, cargo, version, state).await
          {
            send(StateStream::new_cargo_error(&key, &err.to_string()), sx);
            return;
          }
          let res = utils::cargo::start_by_key(&key, state).await;
          if let Err(err) = res {
            send(StateStream::new_cargo_error(&key, &err.to_string()), sx);
            return;
          }
        }
      };
      let key_ptr = key.clone();
      let state_ptr = state.clone();
      rt::spawn(async move {
        let cargo = utils::cargo::inspect_by_key(&key_ptr, &state_ptr)
          .await
          .unwrap();
        let _ = state_ptr
          .event_emitter
          .emit(Event::CargoPatched(Box::new(cargo)))
          .await;
      });
      send(StateStream::new_cargo_success(&key), sx);
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// ## Apply VMS
///
/// This will apply a list of VMs to the system.
/// It will create or update VMs as needed.
///
/// ## Arguments
///
/// * [namespace](str) - The namespace to apply the VMs to
/// * [data](Vec<VmConfigPartial>) - The VMs to apply
/// * [version](str) - The version of the VMs
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender) - The response sender
///
pub(crate) async fn apply_vms(
  namespace: &str,
  data: &[VmConfigPartial],
  version: &str,
  state: &DaemonState,
  qs: &StateApplyQuery,
  sx: &mpsc::Sender<HttpResult<Bytes>>,
) {
  data
    .iter()
    .map(|vm| async {
      let key = utils::key::gen_key(namespace, &vm.name);
      send(StateStream::new_vm_pending(&key), sx);
      match utils::vm::inspect_by_key(&key, &state.docker_api, &state.pool)
        .await
      {
        Ok(existing) => {
          let existing: VmConfigPartial = existing.into();
          let vm = VmConfigPartial {
            disk: VmDiskConfig {
              image: format!("{}.{}", vm.disk.image, &key),
              size: Some(vm.disk.size.unwrap_or(20)),
            },
            host_config: Some(vm.host_config.clone().unwrap_or_default()),
            ..vm.clone()
          };
          if existing == vm && !qs.reload.unwrap_or(false) {
            send(StateStream::new_vm_unchanged(&key), sx);
            return;
          }
          if let Err(err) = utils::vm::put(&key, &vm, version, state).await {
            send(StateStream::new_vm_error(&key, &err.to_string()), sx);
            return;
          }
        }
        Err(_err) => {
          if let Err(err) =
            utils::vm::create(vm, namespace, version, state).await
          {
            send(StateStream::new_vm_error(&key, &err.to_string()), sx);
            return;
          }
          let res = utils::vm::start_by_key(&key, state).await;
          if let Err(err) = res {
            send(StateStream::new_vm_error(&key, &err.to_string()), sx);
            return;
          }
        }
      };
      send(StateStream::new_vm_success(&key), sx);
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// ## Apply resources
///
/// Apply the list of resources to the system.
/// It will create the resources if they don't exist or update them if they are not up to date.
///
/// ## Arguments
///
/// * [data](Vec<ResourcePartial>) - The list of resources to apply
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender) - The response sender
///
async fn apply_resources(
  data: &[ResourcePartial],
  state: &DaemonState,
  qs: &StateApplyQuery,
  sx: &mpsc::Sender<HttpResult<Bytes>>,
) {
  data
    .iter()
    .map(|resource| async {
      let key = resource.name.to_owned();
      send(StateStream::new_resource_pending(&key), sx);
      let res =
        match repositories::resource::inspect_by_key(&key, &state.pool).await {
          Err(_) => utils::resource::create(resource, &state.pool).await,
          Ok(cur_resource) => {
            let casted: ResourcePartial = cur_resource.into();
            if *resource == casted && !qs.reload.unwrap_or(false) {
              send(StateStream::new_resource_unchanged(&key), sx);
              return;
            }
            utils::resource::patch(&resource.clone(), &state.pool).await
          }
        };
      if let Err(err) = res {
        send(StateStream::new_resource_error(&key, &err.to_string()), sx);
        return;
      }
      let key_ptr = key.to_owned();
      let pool_ptr = state.pool.clone();
      let event_emitter = state.event_emitter.clone();
      rt::spawn(async move {
        let resource =
          repositories::resource::inspect_by_key(&key_ptr, &pool_ptr)
            .await
            .unwrap();
        let _ = event_emitter
          .emit(Event::ResourcePatched(Box::new(resource)))
          .await;
      });
      send(StateStream::new_resource_success(&key), sx);
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// ## Remove jobs
///
/// Remove jobs from the system based on a list of jobs
///
/// ## Arguments
///
/// * [data](serde_json::Value) - The state payload
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
async fn remove_jobs(
  data: &[JobPartial],
  state: &DaemonState,
  sx: &mpsc::Sender<Result<Bytes, HttpError>>,
) {
  data
    .iter()
    .map(|job| async {
      send(StateStream::new_job_pending(&job.name), sx);
      match utils::job::inspect_by_name(&job.name, state).await {
        Ok(_) => {
          if let Err(err) = utils::job::delete_by_name(&job.name, state).await {
            send(StateStream::new_job_error(&job.name, &err.to_string()), sx);
            return;
          }
        }
        Err(_err) => {
          send(StateStream::new_job_not_found(&job.name), sx);
          return;
        }
      };
      send(StateStream::new_job_success(&job.name), sx);
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// ## Remove secrets
///
/// Delete secrets from the system based on a list of secrets
///
/// ## Arguments
///
/// * [data](Vec<SecretPartial>) - The list of secrets to delete
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Return
///
/// * [Result](Result) - The result of the operation
///   * [Ok](()) - The operation was successful
///   * [Err](HttpError) - An http response error if something went wrong
///
async fn remove_secrets(
  data: &[SecretPartial],
  state: &DaemonState,
  sx: &mpsc::Sender<Result<Bytes, HttpError>>,
) {
  data
    .iter()
    .map(|secret| async {
      let key = secret.key.to_owned();
      send(StateStream::new_secret_pending(&key), sx);
      let secret =
        match repositories::secret::find_by_key(&key, &state.pool).await {
          Ok(secret) => secret,
          Err(_) => {
            send(StateStream::new_secret_not_found(&key), sx);
            return;
          }
        };
      if let Err(err) =
        repositories::secret::delete_by_key(&secret.key, &state.pool).await
      {
        send(StateStream::new_secret_error(&key, &err.to_string()), sx);
        return;
      }
      let secret_ptr = secret.clone();
      let event_emitter = state.event_emitter.clone();
      rt::spawn(async move {
        let _ = event_emitter
          .emit(Event::SecretDeleted(Box::new(secret_ptr.into())))
          .await;
      });
      send(StateStream::new_secret_success(&key), sx);
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// ## Remove cargoes
///
/// Delete cargoes from the system based on a list of cargoes for a namespace
///
/// ## Arguments
///
/// * [namespace](str) - The namespace of the cargoes
/// * [data](Vec<CargoConfigPartial>) - The list of cargoes to delete
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Return
///
/// * [Result](Result) - The result of the operation
///   * [Ok](()) - The operation was successful
///   * [Err](HttpError) - An http response error if something went wrong
///
async fn remove_cargoes(
  namespace: &str,
  data: &[CargoConfigPartial],
  state: &DaemonState,
  sx: &mpsc::Sender<Result<Bytes, HttpError>>,
) {
  data
    .iter()
    .map(|cargo| async {
      let key = utils::key::gen_key(namespace, &cargo.name);
      send(StateStream::new_cargo_pending(&key), sx);
      let cargo = match utils::cargo::inspect_by_key(&key, state).await {
        Ok(cargo) => cargo,
        Err(_) => {
          send(StateStream::new_cargo_not_found(&key), sx);
          return;
        }
      };
      if let Err(err) =
        utils::cargo::delete_by_key(&key, Some(true), state).await
      {
        send(StateStream::new_cargo_error(&key, &err.to_string()), sx);
        return;
      }
      send(StateStream::new_cargo_success(&key), sx);
      let event_emitter = state.event_emitter.clone();
      rt::spawn(async move {
        let _ = event_emitter
          .emit(Event::CargoDeleted(Box::new(cargo)))
          .await;
      });
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// ## Remove VMs
///
/// This will delete a list of VMs from the system for the given namespace.
///
/// ## Arguments
///
/// * [namespace](str) - The namespace to delete the VMs from
/// * [data](Vec<VmConfigPartial>) - The VMs to delete
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
pub(crate) async fn remove_vms(
  namespace: &str,
  data: &[VmConfigPartial],
  state: &DaemonState,
  sx: &mpsc::Sender<Result<Bytes, HttpError>>,
) {
  data
    .iter()
    .map(|vm| async {
      let key = utils::key::gen_key(namespace, &vm.name);
      send(StateStream::new_vm_pending(&key), sx);
      let res =
        utils::vm::inspect_by_key(&key, &state.docker_api, &state.pool).await;
      if res.is_err() {
        send(StateStream::new_vm_not_found(&key), sx);
        return;
      }
      if let Err(err) = utils::vm::delete_by_key(&key, true, state).await {
        send(StateStream::new_vm_error(&key, &err.to_string()), sx);
        return;
      }
      send(StateStream::new_vm_success(&key), sx);
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// ## Remove resources
///
/// Delete resources from the system based on a list of resources
///
/// ## Arguments
///
/// * [data](Vec<ResourcePartial>) - The list of resources to delete
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
async fn remove_resources(
  data: &[ResourcePartial],
  state: &DaemonState,
  sx: &mpsc::Sender<Result<Bytes, HttpError>>,
) {
  data
    .iter()
    .map(|resource| async {
      send(StateStream::new_resource_pending(&resource.name), sx);
      let resource = match repositories::resource::inspect_by_key(
        &resource.name,
        &state.pool,
      )
      .await
      {
        Ok(resource) => resource,
        Err(_) => {
          send(StateStream::new_resource_not_found(&resource.name), sx);
          return;
        }
      };
      if let Err(err) = utils::resource::delete(&resource, &state.pool).await {
        send(
          StateStream::new_resource_error(&resource.name, &err.to_string()),
          sx,
        );
        return;
      }
      let resource_ptr = resource.clone();
      let event_emitter = state.event_emitter.clone();
      rt::spawn(async move {
        let _ = event_emitter
          .emit(Event::ResourceDeleted(Box::new(resource_ptr)))
          .await;
      });
      send(StateStream::new_resource_success(&resource.name), sx);
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// ## Apply statefile
///
/// Apply a Statefile in the system
///
/// /// ## Arguments
///
/// * [data](StateFileData) - The Statefile data
/// * [state](DaemonState) - The system state
///
/// ## Return
///
/// [Receiver](mpsc::Receiver) of [HttpResult](HttpResult) of [Bytes](Bytes)
///
pub(crate) fn apply_statefile(
  data: &StateFileData,
  version: &str,
  qs: &StateApplyQuery,
  state: &DaemonState,
) -> mpsc::Receiver<HttpResult<Bytes>> {
  let state = state.clone();
  let version = version.to_owned();
  let data = data.clone();
  let qs = qs.clone();
  let (sx, rx) = mpsc::channel::<Result<Bytes, HttpError>>();
  rt::spawn(async move {
    match data {
      StateFileData::Deployment(data) => {
        let _ = ensure_namespace_existence(&data.namespace, &state).await;
        let namespace = data.namespace.unwrap_or("global".to_owned());
        if let Some(secrets) = &data.secrets {
          apply_secrets(secrets, &state, &qs, &sx).await;
        }
        if let Some(cargoes) = &data.cargoes {
          apply_cargoes(&namespace, cargoes, &version, &state, &qs, &sx).await;
        }
        if let Some(vms) = &data.virtual_machines {
          apply_vms(&namespace, vms, &version, &state, &qs, &sx).await;
        }
        if let Some(resources) = &data.resources {
          apply_resources(resources, &state, &qs, &sx).await;
        }
      }
      StateFileData::Cargo(data) => {
        let _ = ensure_namespace_existence(&data.namespace, &state).await;
        apply_cargoes(
          &data.namespace.unwrap_or("global".to_owned()),
          &data.cargoes,
          &version,
          &state,
          &qs,
          &sx,
        )
        .await;
      }
      StateFileData::VirtualMachine(data) => {
        let _ = ensure_namespace_existence(&data.namespace, &state).await;
        apply_vms(
          &data.namespace.unwrap_or("global".to_owned()),
          &data.virtual_machines,
          &version,
          &state,
          &qs,
          &sx,
        )
        .await;
      }
      StateFileData::Resource(data) => {
        apply_resources(&data.resources, &state, &qs, &sx).await;
      }
      StateFileData::Secret(data) => {
        apply_secrets(&data.secrets, &state, &qs, &sx).await;
      }
      StateFileData::Job(data) => {
        apply_jobs(&data.jobs, &state, &qs, &sx).await;
      }
    };
  });
  rx
}

/// ## Remove statefile
///
/// Remove a Statefile from the system and return a stream of the result for
///
/// ## Arguments
///
/// * [data](StateFileData) - The Statefile data
/// * [state](DaemonState) - The system state
///
/// ## Return
///
/// [Receiver](mpsc::Receiver) of [HttpResult](HttpResult) of [Bytes](Bytes)
///
pub(crate) fn remove_statefile(
  data: &StateFileData,
  state: &DaemonState,
) -> mpsc::Receiver<HttpResult<Bytes>> {
  let data = data.clone();
  let state = state.clone();
  let (sx, rx) = mpsc::channel::<Result<Bytes, HttpError>>();
  rt::spawn(async move {
    match data {
      StateFileData::Deployment(data) => {
        let namespace = utils::key::resolve_nsp(&data.namespace);
        if let Some(cargoes) = &data.cargoes {
          remove_cargoes(&namespace, cargoes, &state, &sx).await;
        }
        if let Some(vms) = &data.virtual_machines {
          remove_vms(&namespace, vms, &state, &sx).await;
        }
        if let Some(resources) = &data.resources {
          remove_resources(resources, &state, &sx).await;
        }
        if let Some(secrets) = &data.secrets {
          remove_secrets(secrets, &state, &sx).await;
        }
      }
      StateFileData::Cargo(data) => {
        let namespace = utils::key::resolve_nsp(&data.namespace);
        remove_cargoes(&namespace, &data.cargoes, &state, &sx).await;
      }
      StateFileData::VirtualMachine(data) => {
        let namespace = utils::key::resolve_nsp(&data.namespace);
        remove_vms(&namespace, &data.virtual_machines, &state, &sx).await;
      }
      StateFileData::Resource(data) => {
        remove_resources(&data.resources, &state, &sx).await
      }
      StateFileData::Secret(data) => {
        remove_secrets(&data.secrets, &state, &sx).await
      }
      StateFileData::Job(data) => remove_jobs(&data.jobs, &state, &sx).await,
    };
  });
  rx
}
