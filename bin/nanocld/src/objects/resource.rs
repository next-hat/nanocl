use std::{
  future::Future,
  sync::{Arc, OnceLock},
};

use nanocl_error::{
  http::{HttpError, HttpResult},
  io::IoError,
};
use nanocl_stubs::{
  resource::{Resource, ResourcePartial},
  system::NativeEventAction,
};

use crate::{
  models::{
    Pool, ResourceDb, ResourceMutationLock, ResourceMutationLocks, SystemState,
  },
  repositories::generic::*,
};

use super::generic::*;

fn mutation_lock(key: &str) -> ResourceMutationLock {
  static LOCKS: OnceLock<ResourceMutationLocks> = OnceLock::new();
  let mut locks = LOCKS
    .get_or_init(Default::default)
    .lock()
    .unwrap_or_else(|error| error.into_inner());
  locks.retain(|_, lock| lock.strong_count() > 0);
  if let Some(lock) = locks.get(key).and_then(std::sync::Weak::upgrade) {
    return lock;
  }
  let lock = Arc::new(tokio::sync::Mutex::new(()));
  locks.insert(key.to_owned(), Arc::downgrade(&lock));
  lock
}

impl ResourceDb {
  /// Order this daemon's mutations from their reads through controller apply,
  /// canonical publication and completion events. The owner outlives its waiter;
  /// this is not a cross-daemon or crash-recovery transaction.
  pub(crate) async fn mutate<T: 'static>(
    key: &str,
    mutation: impl Future<Output = HttpResult<T>> + 'static,
  ) -> HttpResult<T> {
    let serialized = mutation_lock(key).lock_owned().await;
    let (reply, response) = tokio::sync::oneshot::channel();
    ntex::rt::spawn(async move {
      let serialized = serialized;
      let result = mutation.await;
      drop(serialized);
      if let Err(Err(error)) = reply.send(result) {
        log::error!(
          "Resource mutation failed after its caller stopped waiting: {error}"
        );
      }
    });
    response.await.map_err(|_| {
      HttpError::internal_server_error(
        "Resource mutation owner stopped before replying",
      )
    })?
  }

  async fn publication_failed(
    candidate: &ResourcePartial,
    error: IoError,
    pool: &Pool,
  ) -> HttpError {
    // COMMIT errors may be ambiguous. Read the canonical revision again rather
    // than assuming the old revision is still current.
    let restore = async {
      match Self::transform_read_by_pk(&candidate.name, pool).await {
        Ok(current) => {
          let accepted = ResourcePartial {
            name: current.spec.resource_key,
            kind: format!("{}/{}", current.kind, current.spec.version),
            data: current.spec.data,
            metadata: current.spec.metadata,
          };
          Self::hook_create(&accepted, pool).await?;
        }
        Err(read) if read.inner.kind() == std::io::ErrorKind::NotFound => {
          let (kind, version) = Self::parse_kind(&candidate.kind, pool).await?;
          Self::hook_delete(
            &Resource {
              created_at: chrono::Utc::now().naive_utc(),
              kind: format!("{kind}/{version}"),
              spec: nanocl_stubs::resource::ResourceSpec {
                key: uuid::Uuid::nil(),
                resource_key: candidate.name.clone(),
                version,
                created_at: chrono::Utc::now().naive_utc(),
                data: candidate.data.clone(),
                metadata: candidate.metadata.clone(),
              },
            },
            pool,
          )
          .await?;
        }
        Err(read) => return Err(read.into()),
      }
      Ok::<_, HttpError>(())
    }
    .await;
    let mut error: HttpError = error.into();
    match restore {
      Ok(()) => error
        .msg
        .push_str("; controller reconciled to canonical Resource state"),
      Err(restore) => error.msg.push_str(&format!(
        "; controller reconciliation remains unresolved: {restore}"
      )),
    }
    error
  }
}

impl ObjCreate for ResourceDb {
  type ObjCreateIn = ResourcePartial;
  type ObjCreateOut = Resource;

  async fn fn_create_obj(
    obj: &Self::ObjCreateIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjCreateOut> {
    match ResourceDb::transform_read_by_pk(&obj.name, &state.inner.pool).await {
      Ok(_) => {
        return Err(HttpError::conflict(format!(
          "Resource {} already exists",
          &obj.name
        )));
      }
      Err(error) if error.inner.kind() == std::io::ErrorKind::NotFound => (),
      Err(error) => return Err(error.into()),
    }
    let obj = ResourceDb::hook_create(obj, &state.inner.pool).await?;
    let publication = async {
      #[cfg(test)]
      tests::before_publication(&obj.name).await?;
      ResourceDb::create_from_spec(&obj, &state.inner.pool).await
    }
    .await;
    let resource = match publication {
      Ok(resource) => resource,
      Err(error) => {
        return Err(
          Self::publication_failed(&obj, error, &state.inner.pool).await,
        );
      }
    };
    Ok(resource)
  }
}

impl ObjDelByPk for ResourceDb {
  type ObjDelOut = Resource;
  type ObjDelOpts = ();

  async fn fn_del_obj_by_pk(
    key: &str,
    _opts: &Self::ObjDelOpts,
    state: &SystemState,
  ) -> HttpResult<Self::ObjDelOut> {
    let resource =
      ResourceDb::transform_read_by_pk(key, &state.inner.pool).await?;
    // Retain the declaration and history if the controller is unavailable, so
    // the normal DELETE can be retried after it returns.
    ResourceDb::hook_delete(&resource, &state.inner.pool).await?;
    ResourceDb::delete_with_history(
      &resource.spec.resource_key,
      &state.inner.pool,
    )
    .await?;
    Ok(resource)
  }
}

impl ObjPutByPk for ResourceDb {
  type ObjPutIn = ResourcePartial;
  type ObjPutOut = Resource;

  fn get_put_event() -> NativeEventAction {
    NativeEventAction::Update
  }

  async fn fn_put_obj_by_pk(
    pk: &str,
    obj: &Self::ObjPutIn,
    state: &SystemState,
  ) -> HttpResult<Self::ObjPutOut> {
    ResourceDb::read_by_pk(pk, &state.inner.pool).await?;
    let resource = ResourceDb::hook_create(obj, &state.inner.pool).await?;
    let publication = async {
      #[cfg(test)]
      tests::before_publication(&resource.name).await?;
      ResourceDb::update_from_spec(&resource, &state.inner.pool).await
    }
    .await;
    match publication {
      Ok(resource) => Ok(resource),
      Err(error) => {
        Err(Self::publication_failed(&resource, error, &state.inner.pool).await)
      }
    }
  }
}

#[cfg(test)]
mod tests;
