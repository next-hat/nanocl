use std::path::Path;
use std::collections::HashMap;

use ntex::web;
use ntex::http::StatusCode;
use diesel::PgConnection;
use diesel::r2d2::ConnectionManager;
use bollard::{
  Docker,
  models::HostConfig,
  errors::Error as DockerError,
  container::{CreateContainerOptions, Config},
  service::{RestartPolicy, RestartPolicyNameEnum},
};

use nanocl_models::config::DaemonConfig;
use nanocl_models::cargo_config::CargoConfigPartial;

use crate::{utils, repositories};
use crate::error::{DaemonError, HttpResponseError};
use crate::models::{Pool, DBConn, ArgState};

/// ## Generate store host config
///
/// Generate a host config struct for the store container
///
/// ## Arguments
///
/// [config](DaemonConfig) Daemon config reference
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](HostConfig) - The host config has been generated
///   - [Err](DaemonError) - The host config has not been generated
///
/// ## Example
///
/// ```rust,norun
/// use nanocl_models::config::DaemonConfig;
///
/// let config = DaemonConfig::default();
/// let host_config = gen_store_host_conf(&config);
/// ```
///
fn gen_store_host_conf(config: &DaemonConfig) -> HostConfig {
  let path = Path::new(&config.state_dir).join("store/data");

  let binds = vec![format!("{}:/cockroach/cockroach-data", path.display())];

  HostConfig {
    binds: Some(binds),
    restart_policy: Some(RestartPolicy {
      name: Some(RestartPolicyNameEnum::UNLESS_STOPPED),
      maximum_retry_count: None,
    }),
    network_mode: Some(String::from("system-nano-internal0")),
    ..Default::default()
  }
}

/// ## Generate a cargo config for the store
///
/// The store is a cockroachdb instance
/// It will generate a cargo for our store to register it in the system namespace
///
/// ## Arguments
///
/// [name](str) The name of the container
/// [config](DaemonConfig) Reference to daemon config
///
/// ## Returns
///
/// - [Result](Result) - The result of the operation
///   - [Ok](CargoConfigPartial) - The cargo config has been generated
///   - [Err](DaemonError) - The cargo config has not been generated
///
/// ## Example
///
/// ```rust,norun
/// use nanocl_models::config::DaemonConfig;
///
/// let config = DaemonConfig::default();
/// let cargo_config = gen_store_cargo_conf("system-store", &config);
/// ```
///
fn gen_store_cargo_conf(
  name: &str,
  config: &DaemonConfig,
) -> CargoConfigPartial {
  let mut labels = HashMap::new();
  labels.insert("io.nanocl.cargo".into(), name.into());
  labels.insert("io.nanocl.namespace".into(), "system".into());
  let host_config = Some(gen_store_host_conf(config));
  CargoConfigPartial {
    name: name.into(),
    dns_entry: None,
    replication: None,
    container: Config {
      image: Some("cockroachdb/cockroach:v21.2.17".into()),
      labels: Some(labels.to_owned()),
      host_config,
      cmd: Some(vec!["start-single-node".into(), "--insecure".into()]),
      ..Default::default()
    },
  }
}

/// ## Create pool
///
/// Create a pool connection to cockroachdb
///
/// ## Arguments
///
/// [host](String) Host to connect to
///
/// ## Returns
///
/// - [Pool](Pool) R2d2 pool connection for postgres
///
/// ## Example
///
/// ```rust,norun
/// use crate::utils;
///
/// let pool = utils::create_pool("localhost".to_string()).await;
/// ```
///
pub async fn create_pool(host: String) -> Pool {
  web::block(move || {
    let db_url =
      "postgres://root:root@".to_owned() + &host + ":26257/defaultdb";
    let manager = ConnectionManager::<PgConnection>::new(db_url);
    r2d2::Pool::builder().build(manager)
  })
  .await
  .expect("cannot connect to postgresql.")
}

/// ## Get connection from the pool
///
/// Get connection from the connection pool
///
/// ## Arguments
///
/// [pool](Pool) a pool wrapped in ntex State
///
/// ## Returns
///
/// - [Result](Result) Result of the operation
///   - [Ok](DBConn) - The connection has been retrieved
///   - [Err](HttpResponseError) - The connection has not been retrieved
///
/// ## Example
///
/// ```rust,norun
/// use crate::utils;
///
/// let pool = utils::store::create_pool("localhost".to_string()).await;
/// let conn = utils::store::get_pool_conn(&pool);
/// ```
///
pub fn get_pool_conn(pool: &Pool) -> Result<DBConn, HttpResponseError> {
  let conn = match pool.get() {
    Ok(conn) => conn,
    Err(err) => {
      return Err(HttpResponseError {
        msg: format!("Cannot get connection from pool got error: {}", &err),
        status: StatusCode::INTERNAL_SERVER_ERROR,
      });
    }
  };
  Ok(conn)
}

/// ## Get store ip address
///
/// Get the ip address of the store container
///
/// ## Arguments
///
/// [docker_api](Docker) Reference to docker api
///
/// ## Returns
///
/// - [Result](Result) Result of the operation
///   - [Ok](String) - The ip address of the store
///   - [Err](HttpResponseError) - The ip address of the store has not been retrieved
///
/// ## Example
///
/// ```rust,norun
/// use crate::utils;
///
/// let docker_api = Docker::connect_with_local_defaults().unwrap();
/// let ip_address = utils::store::get_store_ip_addr(&docker_api).await;
/// ```
///
pub async fn get_store_ip_addr(
  docker_api: &Docker,
) -> Result<String, HttpResponseError> {
  let container = docker_api.inspect_container("system-store", None).await?;
  let networks = container
    .network_settings
    .ok_or(HttpResponseError {
      msg: String::from("unable to get store network nettings"),
      status: StatusCode::INTERNAL_SERVER_ERROR,
    })?
    .networks
    .ok_or(HttpResponseError {
      msg: String::from("unable to get store networks"),
      status: StatusCode::INTERNAL_SERVER_ERROR,
    })?;
  let ip_address = networks
    .get("system-nano-internal0")
    .ok_or(HttpResponseError {
      msg: String::from("unable to get store network nanocl"),
      status: StatusCode::INTERNAL_SERVER_ERROR,
    })?
    .ip_address
    .as_ref()
    .ok_or(HttpResponseError {
      msg: String::from("unable to get store network nanocl"),
      status: StatusCode::INTERNAL_SERVER_ERROR,
    })?;
  Ok(ip_address.to_owned())
}

/// ## Ensure store is running
///
/// Verify is store is running and boot it if not
///
/// ## Arguments
///
/// [config](DaemonConfig) Reference to Daemon config
/// [docker_api](Docker) Reference to docker
///
/// ## Returns
///
/// - [Result](Result) Result of the operation
///   - [Ok](()) - The store is running
///   - [Err](DockerError) - The store is not running
///
/// ## Example
///
/// ```rust,norun
/// use nanocl_models::config::DaemonConfig;
/// use crate::utils;
///
/// let config = DaemonConfig::default();
/// let docker_api = bollard::Docker::connect_with_local_defaults().unwrap();
/// let result = utils::store::boot(&config, &docker_api).await;
/// ```
///
pub async fn boot(
  config: &DaemonConfig,
  docker_api: &Docker,
) -> Result<(), DockerError> {
  let container_name = "system-store";

  if docker_api
    .inspect_container(container_name, None)
    .await
    .is_ok()
  {
    return Ok(());
  }
  let options = Some(CreateContainerOptions {
    name: container_name,
    ..Default::default()
  });
  let config = gen_store_cargo_conf(container_name, config);
  let container = docker_api
    .create_container(options, config.container)
    .await?;
  docker_api
    .start_container::<String>(&container.id, None)
    .await?;
  Ok(())
}

/// ## Register store
///
/// Register store container as a cargo in the database
///
///
/// ## Arguments
///
/// - [arg](ArgState) Reference to argument state
///
/// ## Returns
///
/// - [Result](Result) Result of the operation
///   - [Ok](()) - The store has been registered
///   - [Err](DaemonError) - The store has not been registered
///
/// ## Example
///
/// ```rust,norun
/// use crate::utils;
/// use crate::models::ArgState;
///
/// let arg = ArgState::default();
/// let result = utils::store::register(&arg).await;
/// ```
///
pub async fn register(arg: &ArgState) -> Result<(), DaemonError> {
  let name = "store";
  let key = utils::key::gen_key(&arg.sys_namespace, name);
  if repositories::cargo::find_by_key(key, &arg.pool)
    .await
    .is_ok()
  {
    return Ok(());
  }
  let config = gen_store_cargo_conf(name, &arg.config);
  repositories::cargo::create(arg.sys_namespace.to_owned(), config, &arg.pool)
    .await?;

  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_gen_store_cargo_conf() {
    let config = DaemonConfig::default();
    let store_config = gen_store_cargo_conf("store", &config);
    assert_eq!(store_config.name, "store");
    assert_eq!(
      store_config.container.image,
      Some("cockroachdb/cockroach:v21.2.17".into())
    );
  }
}
