use ntex::{rt, http};
use ntex::util::Bytes;
use ntex::channel::mpsc;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;

use nanocl_error::http::HttpError;

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
use crate::models::{StateData, DaemonState};

/// ## Ensure namespace existence
///
/// Ensure that the namespace exists in the system
///
/// ## Arguments
///
/// * [namespace](Option) - The optional [namespace name](String)
/// * [state](DaemonState) - The system state
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Ok) - [Namespace name](String) if successful
///   * [Err](Err) - [Http error](HttpError) if something went wrong
///
async fn ensure_namespace_existence(
  namespace: &Option<String>,
  state: &DaemonState,
) -> Result<String, HttpError> {
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
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Bytes) - The bytes to send to the client
///   * [Err](HttpError) - An http response error if something went wrong
///
fn stream_to_bytes(state_stream: StateStream) -> Result<Bytes, HttpError> {
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
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
fn send(
  state_stream: StateStream,
  sx: &mpsc::Sender<Result<Bytes, HttpError>>,
) {
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
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](StateData) - The state data
///   * [Err](HttpError) - An http response error if something went wrong
///
pub fn parse_state(data: &serde_json::Value) -> Result<StateData, HttpError> {
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
      Ok(StateData::Deployment(data))
    }
    "Cargo" => {
      let data = serde_json::from_value::<StateCargo>(data.to_owned())
        .map_err(|err| HttpError {
          status: http::StatusCode::BAD_REQUEST,
          msg: format!("unable to serialize payload {err}"),
        })?;
      Ok(StateData::Cargo(data))
    }
    "VirtualMachine" => {
      let data = serde_json::from_value::<StateVirtualMachine>(data.to_owned())
        .map_err(|err| HttpError {
          status: http::StatusCode::BAD_REQUEST,
          msg: format!("unable to serialize payload {err}"),
        })?;
      Ok(StateData::VirtualMachine(data))
    }
    "Resource" => {
      let data = serde_json::from_value::<StateResource>(data.to_owned())
        .map_err(|err| HttpError {
          status: http::StatusCode::BAD_REQUEST,
          msg: format!("unable to serialize payload {err}"),
        })?;
      Ok(StateData::Resource(data))
    }
    "Job" => {
      let data =
        serde_json::from_value::<StateJob>(data.to_owned()).map_err(|err| {
          HttpError {
            status: http::StatusCode::BAD_REQUEST,
            msg: format!("unable to serialize payload {err}"),
          }
        })?;
      Ok(StateData::Job(data))
    }
    "Secret" => {
      let data = serde_json::from_value::<StateSecret>(data.to_owned())
        .map_err(|err| HttpError {
          status: http::StatusCode::BAD_REQUEST,
          msg: format!("unable to serialize payload {err}"),
        })?;
      Ok(StateData::Secret(data))
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
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
async fn apply_secrets(
  data: &[SecretPartial],
  state: &DaemonState,
  qs: &StateApplyQuery,
  sx: &mpsc::Sender<Result<Bytes, HttpError>>,
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

async fn apply_jobs(
  data: &[JobPartial],
  state: &DaemonState,
  qs: &StateApplyQuery,
  sx: &mpsc::Sender<Result<Bytes, HttpError>>,
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
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
async fn apply_cargoes(
  namespace: &str,
  data: &[CargoConfigPartial],
  version: &str,
  state: &DaemonState,
  qs: &StateApplyQuery,
  sx: &mpsc::Sender<Result<Bytes, HttpError>>,
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
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
pub async fn apply_vms(
  namespace: &str,
  data: &[VmConfigPartial],
  version: &str,
  state: &DaemonState,
  qs: &StateApplyQuery,
  sx: &mpsc::Sender<Result<Bytes, HttpError>>,
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
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](()) - The operation was successful
///   * [Err](HttpError) - An http response error if something went wrong
///
async fn apply_resources(
  data: &[ResourcePartial],
  state: &DaemonState,
  qs: &StateApplyQuery,
  sx: &mpsc::Sender<Result<Bytes, HttpError>>,
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

/// ## Apply
///
/// Apply a state payload to the system
///
/// ## Arguments
///
/// * [data](serde_json::Value) - The state payload
/// * [state](DaemonState) - The system state
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
/// ## Returns
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
/// ## Returns
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
pub async fn remove_vms(
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

/// ## Apply Deployment
///
/// Apply a Statefile Kind Deployment to the system.
/// It will create cargoes, vms and ressources or update them if they are not up to date.
///
/// ## Arguments
///
/// * [data](StateDeployment) - The deployment statefile
/// * [version](str) - The version of the deployment
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Ok) - The operation was successful
///   * [Err](Err) - [Http error](HttpError) if something went wrong
///
pub async fn apply_deployment(
  data: &StateDeployment,
  version: &str,
  state: &DaemonState,
  qs: &StateApplyQuery,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  let namespace = ensure_namespace_existence(&data.namespace, state).await?;
  if let Some(secrets) = &data.secrets {
    apply_secrets(secrets, state, qs, &sx).await;
  }
  if let Some(cargoes) = &data.cargoes {
    apply_cargoes(&namespace, cargoes, version, state, qs, &sx).await;
  }
  if let Some(vms) = &data.virtual_machines {
    apply_vms(&namespace, vms, version, state, qs, &sx).await;
  }
  if let Some(resources) = &data.resources {
    apply_resources(resources, state, qs, &sx).await;
  }
  Ok(())
}

/// ## Apply Cargo
///
/// Apply a Statefile Kind Cargo to the system.
/// It will create cargoes or update them if they are not up to date.
///
/// ## Arguments
///
/// * [data](StateCargo) - The cargo statefile
/// * [version](str) - The version of the cargo
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Ok) - The operation was successful
///   * [Err](Err) - [Http error](HttpError) if something went wrong
///
pub async fn apply_cargo(
  data: &StateCargo,
  version: &str,
  state: &DaemonState,
  qs: &StateApplyQuery,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  let namespace = ensure_namespace_existence(&data.namespace, state).await?;
  apply_cargoes(&namespace, &data.cargoes, version, state, qs, &sx).await;
  Ok(())
}

/// ## Apply VM
///
/// This will apply a VM statefile to the system.
/// It will create or update VMs as needed.
///
/// ## Arguments
///
/// * [data](StateVirtualMachine) - The VM statefile data
/// * [version](str) - The version of the VMs
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](()) - The operation was successful
///   * [Err](HttpError) - An http response error if something went wrong
///
pub async fn apply_vm(
  data: &StateVirtualMachine,
  version: &str,
  state: &DaemonState,
  qs: &StateApplyQuery,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  let namespace = ensure_namespace_existence(&data.namespace, state).await?;
  apply_vms(&namespace, &data.virtual_machines, version, state, qs, &sx).await;
  Ok(())
}

/// ## Apply Resource
///
/// Apply a Statefile Kind Resource to the system.
/// It will create resources or update them if they are not up to date.
///
/// ## Arguments
///
/// * [data](StateResource) - The resource statefile
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](()) - The operation was successful
///   * [Err](HttpError) - An http response error if something went wrong
///
pub async fn apply_resource(
  data: &StateResource,
  state: &DaemonState,
  qs: &StateApplyQuery,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  apply_resources(&data.resources, state, qs, &sx).await;
  Ok(())
}

/// ## Apply Secret
///
/// Apply a Statefile Kind Secret to the system.
/// It will create secrets or update them if they are not up to date.
///
/// ## Arguments
///
/// * [data](StateSecret) - The secret Statefile
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](()) - The operation was successful
///   * [Err](HttpError) - An http response error if something went wrong
///
pub async fn apply_secret(
  data: &StateSecret,
  state: &DaemonState,
  qs: &StateApplyQuery,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  apply_secrets(&data.secrets, state, qs, &sx).await;
  Ok(())
}

pub async fn apply_job(
  data: &StateJob,
  state: &DaemonState,
  qs: &StateApplyQuery,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  apply_jobs(&data.jobs, state, qs, &sx).await;
  Ok(())
}

/// ## Remove Deployment
///
/// This will remove all content of a Kind Deployment Statefile from the system.
///
/// ## Arguments
///
/// * [data](StateDeployment) - The deployment statefile data
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](()) - The operation was successful
///   * [Err](HttpError) - An http response error if something went wrong
///
pub async fn remove_deployment(
  data: &StateDeployment,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  let namespace = utils::key::resolve_nsp(&data.namespace);
  if let Some(cargoes) = &data.cargoes {
    remove_cargoes(&namespace, cargoes, state, &sx).await;
  }
  if let Some(vms) = &data.virtual_machines {
    remove_vms(&namespace, vms, state, &sx).await;
  }
  if let Some(resources) = &data.resources {
    remove_resources(resources, state, &sx).await;
  }
  if let Some(secrets) = &data.secrets {
    remove_secrets(secrets, state, &sx).await;
  }
  Ok(())
}

/// ## Remove Cargo
///
/// This will remove all content of a Kind Cargo Statefile from the system.
///
/// ## Arguments
///
/// * [data](StateCargo) - The cargo statefile data
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](()) - The operation was successful
///   * [Err](HttpError) - An http response error if something went wrong
///
pub async fn remove_cargo(
  data: &StateCargo,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  let namespace = utils::key::resolve_nsp(&data.namespace);
  remove_cargoes(&namespace, &data.cargoes, state, &sx).await;
  Ok(())
}

/// ## Remove VM
///
/// This will remove all content of a Kind VirtualMachine Statefile from the system.
///
/// ## Arguments
///
/// * [data](StateVirtualMachine) - The VM statefile data
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](()) - The operation was successful
///   * [Err](HttpError) - An http response error if something went wrong
///
pub async fn remove_vm(
  data: &StateVirtualMachine,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  let namespace = utils::key::resolve_nsp(&data.namespace);
  remove_vms(&namespace, &data.virtual_machines, state, &sx).await;
  Ok(())
}

/// ## Remove Resource
///
/// This will remove all content of a Kind Resource Statefile from the system.
///
/// ## Arguments
///
/// * [data](StateResource) - The resource statefile data
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](()) - The operation was successful
///   * [Err](HttpError) - An http response error if something went wrong
///
pub async fn remove_resource(
  data: &StateResource,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  remove_resources(&data.resources, state, &sx).await;
  Ok(())
}

/// ## Remove secret
///
/// This will remove all content of a Kind Secret Statefile from the system.
///
/// ## Arguments
///
/// * [data](StateSecret) - The secret statefile data
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](()) - The operation was successful
///   * [Err](HttpError) - An http response error if something went wrong
///
pub async fn remove_secret(
  data: &StateSecret,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  remove_secrets(&data.secrets, state, &sx).await;
  Ok(())
}

/// ## Remove Job
///
/// This will remove all content of a Kind Job Statefile from the system.
///
/// ## Arguments
///
/// * [data](StateJob) - The job statefile data
/// * [state](DaemonState) - The system state
/// * [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// * [Result](Result) - The result of the operation
///   * [Ok](Ok) - The operation was successful
///   * [Err](Err) - [Http error](HttpError) if something went wrong
///
pub async fn remove_job(
  data: &StateJob,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  remove_jobs(&data.jobs, state, &sx).await;
  Ok(())
}
