use ntex::rt;
use ntex::http;
use ntex::util::Bytes;
use ntex::channel::mpsc;
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;

use nanocl_utils::http_error::HttpError;

use nanocl_stubs::system::Event;
use nanocl_stubs::resource::ResourcePartial;
use nanocl_stubs::cargo_config::CargoConfigPartial;
use nanocl_stubs::vm_config::{VmConfigPartial, VmDiskConfig};
use nanocl_stubs::state::{
  StateDeployment, StateCargo, StateVirtualMachine, StateResource, StateMeta,
  StateStream,
};

use crate::{utils, repositories};
use crate::models::{StateData, DaemonState};

/// ## Stream to bytes
///
/// Local utility to convert a state stream to bytes to send to the client
///
/// ## Arguments
///
/// - [state_stream](StateStream) - The state stream to convert
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](Bytes) - The bytes to send to the client
///   - [Err](HttpError) - An http response error if something went wrong
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
/// - [state_stream](StateStream) - The state stream to send
/// - [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
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
/// - [data](serde_json::Value) - The state payload
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](StateData) - The state data
///   - [Err](HttpError) - An http response error if something went wrong
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
    _ => Err(HttpError {
      status: http::StatusCode::BAD_REQUEST,
      msg: "unknown type".into(),
    }),
  }
}

/// ## Apply cargoes
///
/// Apply the list of cargoes to the system.
/// It will create the cargoes if they don't exist, and start them.
/// If they exists but are not up to date, it will update them.
///
/// ## Arguments
///
/// - [namespace](str) - The namespace name
/// - [data](Vec<CargoConfigPartial>) - The list of cargoes to apply
/// - [version](str) - The version of the cargoes
/// - [state](DaemonState) - The system state
/// - [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
async fn apply_cargoes(
  namespace: &str,
  data: &[CargoConfigPartial],
  version: &str,
  state: &DaemonState,
  sx: &mpsc::Sender<Result<Bytes, HttpError>>,
) {
  send(
    StateStream::Msg(format!(
      "Applying {} cargoes in namespace: {namespace}",
      data.len(),
    )),
    sx,
  );
  data
    .iter()
    .map(|cargo| async {
      send(
        StateStream::Msg(format!("Applying Cargo {}", cargo.name)),
        sx,
      );
      let key = utils::key::gen_key(namespace, &cargo.name);
      let res = match utils::cargo::inspect_by_key(&key, state).await {
        Ok(existing) => {
          let existing: CargoConfigPartial = existing.into();
          if existing == *cargo {
            send(
              StateStream::Msg(format!(
                "Skipping Cargo {} [NO CHANGE]",
                cargo.name
              )),
              sx,
            );
            return Ok(());
          }
          utils::cargo::put(&key, cargo, version, state).await
        }
        Err(_err) => {
          utils::cargo::create(namespace, cargo, version, state).await
        }
      };
      if let Err(err) = res {
        send(
          StateStream::Error(format!(
            "Unable to apply Cargo {}: {err}",
            cargo.name
          )),
          sx,
        );
        return Ok(());
      }
      send(
        StateStream::Msg(format!("Applied Cargo {}", cargo.name)),
        sx,
      );
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
      let res = utils::cargo::start_by_key(
        &utils::key::gen_key(namespace, &cargo.name),
        state,
      )
      .await;
      if let Err(err) = res {
        send(
          StateStream::Error(format!(
            "Unable to start Cargo: {}: {err}",
            cargo.name
          )),
          sx,
        );
        return Ok(());
      }
      send(
        StateStream::Msg(format!("Started Cargo {}", cargo.name)),
        sx,
      );
      let key_ptr = key.clone();
      let state_ptr = state.clone();
      rt::spawn(async move {
        let cargo = utils::cargo::inspect_by_key(&key_ptr, &state_ptr)
          .await
          .unwrap();
        let _ = state_ptr
          .event_emitter
          .emit(Event::CargoStarted(Box::new(cargo)))
          .await;
      });
      Ok::<_, HttpError>(())
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
/// - [namespace](str) - The namespace to apply the VMs to
/// - [data](Vec<VmConfigPartial>) - The VMs to apply
/// - [version](str) - The version of the VMs
/// - [state](DaemonState) - The system state
/// - [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
pub async fn apply_vms(
  namespace: &str,
  data: &[VmConfigPartial],
  version: &str,
  state: &DaemonState,
  sx: &mpsc::Sender<Result<Bytes, HttpError>>,
) {
  send(
    StateStream::Msg(format!(
      "Applying {} VMs in namespace: {namespace}",
      data.len(),
    )),
    sx,
  );
  data
    .iter()
    .map(|vm| async {
      send(StateStream::Msg(format!("Applying VM: {}", &vm.name)), sx);
      let key = utils::key::gen_key(namespace, &vm.name);
      let res =
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
            if existing == vm {
              send(
                StateStream::Msg(format!(
                  "Skipping VM {} [NO CHANGE]",
                  vm.name
                )),
                sx,
              );
              return Ok(());
            }
            utils::vm::put(&key, &vm, version, state).await
          }
          Err(_err) => utils::vm::create(vm, namespace, version, state).await,
        };
      if let Err(err) = res {
        send(
          StateStream::Error(format!("Failed to apply VM {}: {err}", &vm.name)),
          sx,
        );
        return Ok(());
      }
      send(StateStream::Msg(format!("Applied VM: {}", &vm.name)), sx);
      // TODO: Add event emitter for VM
      if let Err(err) = utils::vm::start_by_key(&key, &state.docker_api).await {
        send(
          StateStream::Error(format!("Failed to start VM {}: {err}", &vm.name)),
          sx,
        );
        return Ok(());
      }
      send(StateStream::Msg(format!("Started VM: {}", &vm.name)), sx);
      Ok::<_, HttpError>(())
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
/// - [data](Vec<ResourcePartial>) - The list of resources to apply
/// - [state](DaemonState) - The system state
/// - [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The operation was successful
///   - [Err](HttpError) - An http response error if something went wrong
///
async fn apply_resources(
  data: &[ResourcePartial],
  state: &DaemonState,
  sx: &mpsc::Sender<Result<Bytes, HttpError>>,
) {
  send(
    StateStream::Msg(format!("Applying {} resources", data.len())),
    sx,
  );
  data
    .iter()
    .map(|resource| async {
      send(
        StateStream::Msg(format!("Applying Resource {}", resource.name)),
        sx,
      );
      let key = resource.name.to_owned();
      let res =
        match repositories::resource::inspect_by_key(&key, &state.pool).await {
          Err(_) => utils::resource::create(resource, &state.pool).await,
          Ok(cur_resource) => {
            let casted: ResourcePartial = cur_resource.into();
            if *resource == casted {
              send(
                StateStream::Msg(format!(
                  "Skipping Resource {} [NO CHANGE]",
                  resource.name
                )),
                sx,
              );
              return Ok(());
            }
            utils::resource::patch(&resource.clone(), &state.pool).await
          }
        };
      if let Err(err) = res {
        send(
          StateStream::Error(format!(
            "Unable to apply Resource {}: {err}",
            resource.name
          )),
          sx,
        );
        return Ok(());
      }
      send(
        StateStream::Msg(format!("Applied Resource {}", resource.name)),
        sx,
      );
      let pool = state.pool.clone();
      let event_emitter = state.event_emitter.clone();
      rt::spawn(async move {
        let resource = repositories::resource::inspect_by_key(&key, &pool)
          .await
          .unwrap();
        let _ = event_emitter
          .emit(Event::ResourcePatched(Box::new(resource)))
          .await;
      });
      Ok::<_, HttpError>(())
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
/// - [namespace](str) - The namespace of the cargoes
/// - [data](Vec<CargoConfigPartial>) - The list of cargoes to delete
/// - [state](DaemonState) - The system state
/// - [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The operation was successful
///   - [Err](HttpError) - An http response error if something went wrong
///
async fn remove_cargoes(
  namespace: &str,
  data: &[CargoConfigPartial],
  state: &DaemonState,
  sx: &mpsc::Sender<Result<Bytes, HttpError>>,
) {
  send(
    StateStream::Msg(format!(
      "Deleting {} cargoes in namespace {namespace}",
      data.len()
    )),
    sx,
  );
  data
    .iter()
    .map(|cargo| async {
      send(
        StateStream::Msg(format!("Deleting Cargo {}", cargo.name)),
        sx,
      );
      let key = utils::key::gen_key(namespace, &cargo.name);
      let cargo = match utils::cargo::inspect_by_key(&key, state).await {
        Ok(cargo) => cargo,
        Err(_) => {
          send(
            StateStream::Msg(format!(
              "Skipping Cargo {} [NOT FOUND]",
              cargo.name
            )),
            sx,
          );
          return Ok(());
        }
      };
      utils::cargo::delete_by_key(&key, Some(true), state).await?;
      send(
        StateStream::Msg(format!("Deleted Cargo {}", cargo.name)),
        sx,
      );
      let event_emitter = state.event_emitter.clone();
      rt::spawn(async move {
        let _ = event_emitter
          .emit(Event::CargoDeleted(Box::new(cargo)))
          .await;
      });
      Ok::<_, HttpError>(())
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
/// - [namespace](str) - The namespace to delete the VMs from
/// - [data](Vec<VmConfigPartial>) - The VMs to delete
/// - [state](DaemonState) - The system state
/// - [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
pub async fn remove_vms(
  namespace: &str,
  data: &[VmConfigPartial],
  state: &DaemonState,
  sx: &mpsc::Sender<Result<Bytes, HttpError>>,
) {
  send(
    StateStream::Msg(format!(
      "Deleting {} VMs in namespace: {namespace}",
      data.len(),
    )),
    sx,
  );
  data
    .iter()
    .map(|vm| async {
      send(StateStream::Msg(format!("Deleting VM: {}", &vm.name)), sx);
      let key = utils::key::gen_key(namespace, &vm.name);
      let res =
        utils::vm::inspect_by_key(&key, &state.docker_api, &state.pool).await;
      if res.is_err() {
        send(
          StateStream::Error(format!("Skiping VM {} [NOT FOUND]", vm.name)),
          sx,
        );
        return Ok(());
      }
      utils::vm::delete_by_key(&key, true, &state.docker_api, &state.pool)
        .await?;
      send(StateStream::Msg(format!("Deleted VM: {}", &vm.name)), sx);
      // Event emitter here
      Ok::<_, HttpError>(())
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
/// - [data](Vec<ResourcePartial>) - The list of resources to delete
/// - [state](DaemonState) - The system state
/// - [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The operation was successful
///   - [Err](HttpError) - An http response error if something went wrong
///
async fn remove_resources(
  data: &[ResourcePartial],
  state: &DaemonState,
  sx: &mpsc::Sender<Result<Bytes, HttpError>>,
) {
  send(
    StateStream::Msg(format!("Deleting {} resources", data.len())),
    sx,
  );
  data
    .iter()
    .map(|resource| async {
      send(
        StateStream::Msg(format!("Deleting Resource {}", resource.name)),
        sx,
      );
      let resource = match repositories::resource::inspect_by_key(
        &resource.name,
        &state.pool,
      )
      .await
      {
        Ok(resource) => resource,
        Err(_) => {
          send(
            StateStream::Msg(format!(
              "Skipping Resource {} [NOT FOUND]",
              resource.name
            )),
            sx,
          );
          return Ok(());
        }
      };
      utils::resource::delete(&resource, &state.pool).await?;
      send(
        StateStream::Msg(format!("Deleted Resource {}", resource.name)),
        sx,
      );
      let event_emitter = state.event_emitter.clone();
      rt::spawn(async move {
        let _ = event_emitter
          .emit(Event::ResourceDeleted(Box::new(resource)))
          .await;
      });
      Ok::<_, HttpError>(())
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
/// - [data](StateDeployment) - The deployment statefile
/// - [version](str) - The version of the deployment
/// - [state](DaemonState) - The system state
/// - [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The operation was successful
///   - [Err](HttpError) - An http response error if something went wrong
///
pub async fn apply_deployment(
  data: &StateDeployment,
  version: &str,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  let namespace = if let Some(namespace) = &data.namespace {
    utils::namespace::create_if_not_exists(namespace, state).await?;
    namespace.to_owned()
  } else {
    "global".into()
  };
  if let Some(cargoes) = &data.cargoes {
    apply_cargoes(&namespace, cargoes, version, state, &sx).await;
  }
  if let Some(vms) = &data.virtual_machines {
    apply_vms(&namespace, vms, version, state, &sx).await;
  }
  if let Some(resources) = &data.resources {
    apply_resources(resources, state, &sx).await;
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
/// - [data](StateCargo) - The cargo statefile
/// - [version](str) - The version of the cargo
/// - [state](DaemonState) - The system state
/// - [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The operation was successful
///   - [Err](HttpError) - An http response error if something went wrong
///
pub async fn apply_cargo(
  data: &StateCargo,
  version: &str,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  let namespace = if let Some(namespace) = &data.namespace {
    utils::namespace::create_if_not_exists(namespace, state).await?;
    namespace.to_owned()
  } else {
    "global".into()
  };
  apply_cargoes(&namespace, &data.cargoes, version, state, &sx).await;
  Ok(())
}

/// ## Apply VM
///
/// This will apply a VM statefile to the system.
/// It will create or update VMs as needed.
///
/// ## Arguments
///
/// - [data](StateVirtualMachine) - The VM statefile data
/// - [version](str) - The version of the VMs
/// - [state](DaemonState) - The system state
/// - [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The operation was successful
///   - [Err](HttpError) - An http response error if something went wrong
///
pub async fn apply_vm(
  data: &StateVirtualMachine,
  version: &str,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  let namespace = if let Some(namespace) = &data.namespace {
    utils::namespace::create_if_not_exists(namespace, state).await?;
    namespace.to_owned()
  } else {
    "global".into()
  };
  apply_vms(&namespace, &data.virtual_machines, version, state, &sx).await;
  Ok(())
}

/// ## Apply Resource
///
/// Apply a Statefile Kind Resource to the system.
/// It will create resources or update them if they are not up to date.
///
/// ## Arguments
///
/// - [data](StateResource) - The resource statefile
/// - [state](DaemonState) - The system state
/// - [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The operation was successful
///   - [Err](HttpError) - An http response error if something went wrong
///
pub async fn apply_resource(
  data: &StateResource,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  apply_resources(&data.resources, state, &sx).await;
  Ok(())
}

/// ## Remove Deployment
///
/// This will remove all content of a Kind Deployment Statefile from the system.
///
/// ## Arguments
///
/// - [data](StateDeployment) - The deployment statefile data
/// - [state](DaemonState) - The system state
/// - [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The operation was successful
///   - [Err](HttpError) - An http response error if something went wrong
///
pub async fn remove_deployment(
  data: &StateDeployment,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  let namespace = if let Some(namespace) = &data.namespace {
    namespace.to_owned()
  } else {
    "global".into()
  };
  if let Some(cargoes) = &data.cargoes {
    remove_cargoes(&namespace, cargoes, state, &sx).await;
  }
  if let Some(vms) = &data.virtual_machines {
    remove_vms(&namespace, vms, state, &sx).await;
  }
  if let Some(resources) = &data.resources {
    remove_resources(resources, state, &sx).await;
  }
  Ok(())
}

/// ## Remove Cargo
///
/// This will remove all content of a Kind Cargo Statefile from the system.
///
/// ## Arguments
///
/// - [data](StateCargo) - The cargo statefile data
/// - [state](DaemonState) - The system state
/// - [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The operation was successful
///   - [Err](HttpError) - An http response error if something went wrong
///
pub async fn remove_cargo(
  data: &StateCargo,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  let namespace = if let Some(namespace) = &data.namespace {
    namespace.to_owned()
  } else {
    "global".into()
  };
  remove_cargoes(&namespace, &data.cargoes, state, &sx).await;
  Ok(())
}

/// ## Remove VM
///
/// This will remove all content of a Kind VirtualMachine Statefile from the system.
///
/// ## Arguments
///
/// - [data](StateVirtualMachine) - The VM statefile data
/// - [state](DaemonState) - The system state
/// - [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The operation was successful
///   - [Err](HttpError) - An http response error if something went wrong
///
pub async fn remove_vm(
  data: &StateVirtualMachine,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  let namespace = if let Some(namespace) = &data.namespace {
    namespace.to_owned()
  } else {
    "global".into()
  };
  remove_vms(&namespace, &data.virtual_machines, state, &sx).await;
  Ok(())
}

/// ## Remove Resource
///
/// This will remove all content of a Kind Resource Statefile from the system.
///
/// ## Arguments
///
/// - [data](StateResource) - The resource statefile data
/// - [state](DaemonState) - The system state
/// - [sx](mpsc::Sender<Result<Bytes, HttpError>>) - The response sender
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](()) - The operation was successful
///   - [Err](HttpError) - An http response error if something went wrong
///
pub async fn remove_resource(
  data: &StateResource,
  state: &DaemonState,
  sx: mpsc::Sender<Result<Bytes, HttpError>>,
) -> Result<(), HttpError> {
  remove_resources(&data.resources, state, &sx).await;
  Ok(())
}
