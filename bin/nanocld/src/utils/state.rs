use ntex::{rt, http};
use ntex::util::Bytes;
use ntex::channel::mpsc;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;

use nanocl_error::http::{HttpError, HttpResult};

use nanocl_stubs::job::JobPartial;
use nanocl_stubs::resource::ResourcePartial;
use nanocl_stubs::secret::{SecretPartial, SecretUpdate};
use nanocl_stubs::cargo_spec::CargoSpecPartial;
use nanocl_stubs::vm_spec::{VmSpecPartial, VmDisk};
use nanocl_stubs::state::{Statefile, StateStream, StateApplyQuery};

use crate::utils;
use crate::models::{
  DaemonState, ResourceDb, SecretDb, Repository, VmDb, CargoDb, ProcessKind,
};

/// Ensure that the namespace exists in the system
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

/// Local utility to convert a state stream to bytes to send to the client
fn stream_to_bytes(state_stream: StateStream) -> HttpResult<Bytes> {
  let bytes =
    serde_json::to_string(&state_stream).map_err(|err| HttpError {
      status: http::StatusCode::INTERNAL_SERVER_ERROR,
      msg: format!("unable to serialize state_stream_to_bytes {err}"),
    })?;
  Ok(Bytes::from(bytes + "\r\n"))
}

/// Send a state stream to the client through the sender channel
fn send(state_stream: StateStream, sx: &mpsc::Sender<HttpResult<Bytes>>) {
  let _ = sx.send(stream_to_bytes(state_stream));
}

/// Parse the state payload and return the data
pub(crate) fn parse_state(data: &serde_json::Value) -> HttpResult<Statefile> {
  let data =
    serde_json::from_value::<Statefile>(data.to_owned()).map_err(|err| {
      HttpError {
        status: http::StatusCode::BAD_REQUEST,
        msg: format!("unable to serialize payload {err}"),
      }
    })?;
  Ok(data)
}

/// Apply the list of secrets to the system.
/// It will create the secrets if they don't exist.
/// If they exists but are not up to date, it will update them.
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
      match SecretDb::find_by_pk(&key, &state.pool).await {
        Ok(existing) => {
          match existing {
            Err(_) => {
              if let Err(err) =
                SecretDb::create(&secret.clone(), &state.pool).await
              {
                send(StateStream::new_secret_error(&key, &err.to_string()), sx);
                return;
              }
            }
            Ok(existing) => {
              let existing: SecretPartial = existing.into();
              if existing == *secret && !qs.reload.unwrap_or(false) {
                send(StateStream::new_secret_unchanged(&key), sx);
                return;
              }
              if let Err(err) = SecretDb::update_by_pk(
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
          };
        }
        Err(err) => {
          send(StateStream::new_secret_error(&key, &err.to_string()), sx);
          return;
        }
      };
      send(StateStream::new_secret_success(&key), sx);
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// Apply the list of jobs to the system.
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
          if let Err(err) =
            utils::process::start_by_kind(&ProcessKind::Job, &job.name, state)
              .await
          {
            send(StateStream::new_job_error(&job.name, &err.to_string()), sx);
            return;
          }
        }
        Err(_err) => {
          if let Err(err) = utils::job::create(job, state).await {
            send(StateStream::new_job_error(&job.name, &err.to_string()), sx);
            return;
          }
          if let Err(err) =
            utils::process::start_by_kind(&ProcessKind::Job, &job.name, state)
              .await
          {
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

/// Apply the list of cargoes to the system.
/// It will create the cargoes if they don't exist, and start them.
/// If they exists but are not up to date, it will update them.
async fn apply_cargoes(
  namespace: &str,
  data: &[CargoSpecPartial],
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
      match CargoDb::inspect_by_pk(&key, &state.pool).await {
        Ok(existing) => {
          let existing: CargoSpecPartial = existing.into();
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
          let res =
            utils::process::start_by_kind(&ProcessKind::Cargo, &key, state)
              .await;
          if let Err(err) = res {
            send(StateStream::new_cargo_error(&key, &err.to_string()), sx);
            return;
          }
        }
      };
      send(StateStream::new_cargo_success(&key), sx);
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// This will apply a list of VMs to the system.
/// It will create or update VMs as needed.
pub(crate) async fn apply_vms(
  namespace: &str,
  data: &[VmSpecPartial],
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
      match VmDb::inspect_by_pk(&key, &state.pool).await {
        Ok(existing) => {
          let existing: VmSpecPartial = existing.into();
          let vm = VmSpecPartial {
            disk: VmDisk {
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
          let res =
            utils::process::start_by_kind(&ProcessKind::Vm, &key, state).await;
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

/// Apply the list of resources to the system.
/// It will create the resources if they don't exist or update them if they are not up to date.
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
      let res = match ResourceDb::inspect_by_pk(&key, &state.pool).await {
        Err(_) => utils::resource::create(resource, state).await,
        Ok(cur_resource) => {
          let casted: ResourcePartial = cur_resource.into();
          if *resource == casted && !qs.reload.unwrap_or(false) {
            send(StateStream::new_resource_unchanged(&key), sx);
            return;
          }
          utils::resource::patch(&resource.clone(), state).await
        }
      };
      if let Err(err) = res {
        send(StateStream::new_resource_error(&key, &err.to_string()), sx);
        return;
      }
      send(StateStream::new_resource_success(&key), sx);
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// Remove jobs from the system based on a list of jobs
async fn remove_jobs(
  data: &[JobPartial],
  state: &DaemonState,
  sx: &mpsc::Sender<HttpResult<Bytes>>,
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

/// Delete secrets from the system based on a list of secrets
async fn remove_secrets(
  data: &[SecretPartial],
  state: &DaemonState,
  sx: &mpsc::Sender<HttpResult<Bytes>>,
) {
  data
    .iter()
    .map(|secret| async {
      let key = secret.key.to_owned();
      send(StateStream::new_secret_pending(&key), sx);
      let secret = match SecretDb::find_by_pk(&key, &state.pool).await {
        Ok(secret) => match secret {
          Ok(secret) => secret,
          Err(_) => {
            send(StateStream::new_secret_not_found(&key), sx);
            return;
          }
        },
        Err(_) => {
          send(StateStream::new_secret_not_found(&key), sx);
          return;
        }
      };
      if let Err(err) = SecretDb::delete_by_pk(&secret.key, &state.pool).await {
        send(StateStream::new_secret_error(&key, &err.to_string()), sx);
        return;
      }
      send(StateStream::new_secret_success(&key), sx);
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// Delete cargoes from the system based on a list of cargoes for a namespace
async fn remove_cargoes(
  namespace: &str,
  data: &[CargoSpecPartial],
  state: &DaemonState,
  sx: &mpsc::Sender<HttpResult<Bytes>>,
) {
  data
    .iter()
    .map(|cargo| async {
      let key = utils::key::gen_key(namespace, &cargo.name);
      send(StateStream::new_cargo_pending(&key), sx);
      let cargo = match CargoDb::inspect_by_pk(&key, &state.pool).await {
        Ok(cargo) => cargo,
        Err(_) => {
          send(StateStream::new_cargo_not_found(&key), sx);
          return;
        }
      };
      if let Err(err) =
        utils::cargo::delete_by_key(&cargo.spec.cargo_key, Some(true), state)
          .await
      {
        send(
          StateStream::new_cargo_error(&cargo.spec.cargo_key, &err.to_string()),
          sx,
        );
        return;
      }
      send(StateStream::new_cargo_success(&cargo.spec.cargo_key), sx);
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// This will delete a list of VMs from the system for the given namespace.
pub(crate) async fn remove_vms(
  namespace: &str,
  data: &[VmSpecPartial],
  state: &DaemonState,
  sx: &mpsc::Sender<HttpResult<Bytes>>,
) {
  data
    .iter()
    .map(|vm| async {
      let key = utils::key::gen_key(namespace, &vm.name);
      send(StateStream::new_vm_pending(&key), sx);
      let res = VmDb::inspect_by_pk(&key, &state.pool).await;
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

/// Delete resources from the system based on a list of resources
async fn remove_resources(
  data: &[ResourcePartial],
  state: &DaemonState,
  sx: &mpsc::Sender<HttpResult<Bytes>>,
) {
  data
    .iter()
    .map(|resource| async {
      send(StateStream::new_resource_pending(&resource.name), sx);
      let resource =
        match ResourceDb::inspect_by_pk(&resource.name, &state.pool).await {
          Ok(resource) => resource,
          Err(_) => {
            send(StateStream::new_resource_not_found(&resource.name), sx);
            return;
          }
        };
      if let Err(err) = utils::resource::delete(&resource, state).await {
        send(
          StateStream::new_resource_error(
            &resource.spec.resource_key,
            &err.to_string(),
          ),
          sx,
        );
        return;
      }
      send(
        StateStream::new_resource_success(&resource.spec.resource_key),
        sx,
      );
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// Apply a Statefile in the system
pub(crate) fn apply_statefile(
  data: &Statefile,
  version: &str,
  qs: &StateApplyQuery,
  state: &DaemonState,
) -> mpsc::Receiver<HttpResult<Bytes>> {
  let state = state.clone();
  let version = version.to_owned();
  let data = data.clone();
  let qs = qs.clone();
  let (sx, rx) = mpsc::channel::<HttpResult<Bytes>>();
  rt::spawn(async move {
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
    if let Some(jobs) = &data.jobs {
      apply_jobs(jobs, &state, &qs, &sx).await;
    }
  });
  rx
}

/// Remove a Statefile from the system and return a stream of the result for
pub(crate) fn remove_statefile(
  data: &Statefile,
  state: &DaemonState,
) -> mpsc::Receiver<HttpResult<Bytes>> {
  let data = data.clone();
  let state = state.clone();
  let (sx, rx) = mpsc::channel::<HttpResult<Bytes>>();
  rt::spawn(async move {
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
    if let Some(jobs) = &data.jobs {
      remove_jobs(jobs, &state, &sx).await
    }
  });
  rx
}
