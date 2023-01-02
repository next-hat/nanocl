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

use nanocl_models::cargo::CargoPartial;
use nanocl_models::cargo_config::CargoConfigPartial;

use crate::{utils, repositories};
use crate::error::{DaemonError, HttpResponseError};
use crate::models::{Pool, DBConn, ArgState, DaemonConfig};

/// Generate HostConfig struct for container creation
///
/// ## Arguments
/// [config](DaemonConfig) Daemon config reference
///
/// ## Returns
/// [HostConfig](HostConfig) HostConfig struct for container creation
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

/// Generate container config for system store
/// This function will generate a container config for the system store
///
/// ## Arguments
/// [name](str) The name of the container
/// [config](DaemonConfig) Reference to daemon config
///
/// ## Returns
/// [Config](Config) The container config
fn gen_store_cargo_conf(
  name: &str,
  config: &DaemonConfig,
) -> CargoConfigPartial {
  let key = utils::key::gen_key("system", name);
  let mut labels = HashMap::new();
  labels.insert("cargo".into(), key);
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

/// Create a connection pool for postgres database
///
/// ## Arguments
/// [host](String) Host to connect to
///
/// ## Returns
/// - [Pool](Pool) R2d2 pool connection for postgres
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

/// Get connection from the connection pool
///
/// ## Arguments
/// [pool](Pool) a pool wrapped in ntex State
///
/// ## Returns
/// - [DBConn](DBConn) A connection to the database
/// - [HttpResponseError](HttpResponseError) Error if unable to get connection
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

/// Get store ip address
///
/// ## Arguments
/// [docker_api](Docker) Reference to docker api
///
/// ## Returns
/// - [String](String) Ip address of the store
/// - [HttpResponseError](HttpResponseError) Error if unable to get ip address
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

/// Boot the store and ensure it's running
///
/// ## Arguments
/// [config](DaemonConfig) Reference to Daemon config
/// [docker_api](Docker) Reference to docker
///
/// ## Returns
/// - [Result](Result) Result of the boot process
/// - [DockerError](DockerError) Error if unable to boot store
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

/// Register store as a cargo
///
/// ## Arguments
/// [arg](ArgState) Reference to argument state
///
/// ## Returns
/// - [Result](Result) Result of the registration process
/// - [DaemonError](DaemonError) Error if unable to register store
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
  let cargo = CargoPartial {
    name: config.name.to_owned(),
    config,
  };
  repositories::cargo::create(arg.sys_namespace.to_owned(), cargo, &arg.pool)
    .await?;

  Ok(())
}
