//! Cargo-scoped, one-shot runtime uncertainty injection for CI regressions.
use std::{
  collections::HashMap,
  sync::{LazyLock, Mutex},
};

use nanocl_error::io::{IoError, IoResult};
use nanocl_stubs::process::Process;

use crate::models::{CargoReplicaProcessDb, CargoRuntimeContext, ProcessDb};

use super::is_candidate_process_name;

enum Fault {
  Cleanup,
  LostCreate(usize),
  Revalidation(usize),
  PreviousSnapshot(usize),
  PreviousMappingRace { remove_runtime: bool },
}
static FAULTS: LazyLock<Mutex<HashMap<String, Fault>>> =
  LazyLock::new(|| Mutex::new(HashMap::new()));

pub(crate) struct Guard {
  key: String,
}
impl Drop for Guard {
  fn drop(&mut self) {
    FAULTS
      .lock()
      .unwrap_or_else(|error| error.into_inner())
      .remove(&self.key);
  }
}

fn install(key: &str, fault: Fault) -> Guard {
  assert!(
    FAULTS
      .lock()
      .unwrap()
      .insert(key.to_owned(), fault)
      .is_none()
  );
  Guard {
    key: key.to_owned(),
  }
}

pub(crate) fn fail_candidate_cleanup(key: &str) -> Guard {
  install(key, Fault::Cleanup)
}

pub(crate) fn lose_second_candidate_create_reply(key: &str) -> Guard {
  install(key, Fault::LostCreate(2))
}

pub(crate) fn fail_candidate_revalidation(
  key: &str,
  after_promotion: bool,
) -> Guard {
  install(
    key,
    Fault::Revalidation(if after_promotion { 2 } else { 1 }),
  )
}

pub(crate) fn remove_previous_after_mapping_snapshot(
  key: &str,
  remove_runtime: bool,
) -> Guard {
  install(key, Fault::PreviousMappingRace { remove_runtime })
}

pub(super) async fn after_mappings_snapshot(
  mappings: &[CargoReplicaProcessDb],
  state: &CargoRuntimeContext,
) -> IoResult<()> {
  let remove_runtime = {
    let mut faults = FAULTS.lock().unwrap();
    if matches!(
      faults.get(&state.operation.resource_key),
      Some(Fault::PreviousMappingRace { .. })
    ) {
      match faults.remove(&state.operation.resource_key) {
        Some(Fault::PreviousMappingRace { remove_runtime }) => {
          Some(remove_runtime)
        }
        _ => unreachable!(),
      }
    } else {
      None
    }
  };
  let Some(remove_runtime) = remove_runtime else {
    return Ok(());
  };
  let ids = mappings
    .iter()
    .filter_map(|mapping| mapping.process_key.as_deref())
    .collect::<Vec<_>>();
  assert_eq!(
    ids.len(),
    1,
    "mapping race requires one captured process attachment"
  );
  // The real foreign key clears the persisted attachment; the caller's already
  // captured mapping still carries the old ID, as in a concurrent destroy event.
  if remove_runtime {
    super::lifecycle::delete_instance(
      ids[0],
      Some(bollard_next::query_parameters::RemoveContainerOptions {
        force: true,
        ..Default::default()
      }),
      state,
    )
    .await
  } else {
    ProcessDb::delete_owned(
      ids[0],
      Some(state.operation.owner()),
      state.lease.clone(),
      &state.inner.pool,
    )
    .await
  }
}

pub(crate) fn fail_previous_snapshot(
  key: &str,
  replica_number: usize,
) -> Guard {
  assert!(replica_number > 0);
  install(key, Fault::PreviousSnapshot(replica_number))
}

pub(super) fn before_previous_snapshot(key: &str) -> IoResult<()> {
  let mut faults = FAULTS.lock().unwrap();
  if let Some(Fault::PreviousSnapshot(remaining)) = faults.get_mut(key) {
    *remaining -= 1;
    if *remaining == 0 {
      faults.remove(key);
      return Err(IoError::interrupted(
        "InspectCargoProcess",
        "previous runtime inspection unavailable",
      ));
    }
  }
  Ok(())
}

pub(super) fn before_revalidation(key: &str) -> IoResult<()> {
  let mut faults = FAULTS.lock().unwrap();
  if let Some(Fault::Revalidation(remaining)) = faults.get_mut(key) {
    *remaining -= 1;
    if *remaining == 0 {
      faults.remove(key);
      return Err(IoError::interrupted(
        "InspectCargoProcess",
        "candidate revalidation unavailable",
      ));
    }
  }
  Ok(())
}

pub(super) fn after_create(process: &Process) -> IoResult<()> {
  let mut faults = FAULTS.lock().unwrap();
  if let Some(Fault::LostCreate(remaining)) = faults.get_mut(&process.kind_key)
  {
    *remaining -= 1;
    if *remaining == 0 {
      faults.remove(&process.kind_key);
      // The caller never receives this ID and therefore cannot add it to its
      // tracked candidate slots, just as with an uncertain Docker create reply.
      return Err(IoError::interrupted(
        "CreateProcess",
        "candidate create response lost",
      ));
    }
  }
  Ok(())
}

pub(super) fn before_cleanup(process: &Process) -> IoResult<()> {
  let mut faults = FAULTS.lock().unwrap();
  if is_candidate_process_name(&process.name)
    && matches!(faults.get(&process.kind_key), Some(Fault::Cleanup))
  {
    faults.remove(&process.kind_key);
    return Err(IoError::interrupted(
      "DeleteProcess",
      "candidate removal not confirmed",
    ));
  }
  Ok(())
}
