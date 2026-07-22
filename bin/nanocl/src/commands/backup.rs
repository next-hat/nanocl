use bollard_next::{container::Config, secret::HostConfig};
use nanocl_error::io::{IoError, IoResult};
use nanocld_client::stubs::{
  cargo_spec::CargoSpecPartial, generic::GenericFilterNsp, job::JobPartial,
  resource::ResourcePartial, secret::SecretPartial, statefile::Statefile,
  vm_spec::VmSpecPartial,
};

use crate::{config::CliConfig, models::BackupOpts, utils};

/// Path inside containers where nanocld automatically mounts TLS secrets.
const NANOCL_SECRET_MOUNT: &str = "/opt/nanocl.io/secrets";

/// Check if a bind mount is the nanocld auto-mounted secret folder.
fn is_secret_bind(bind: &str) -> bool {
  let parts: Vec<&str> = bind.split(':').collect();
  parts.len() >= 2 && parts[1] == NANOCL_SECRET_MOUNT
}

/// Remove nanocld auto-mounted secret binds from a container config.
fn clean_config_binds(config: &Config) -> Config {
  let mut config = config.clone();
  if let Some(host_config) = &config.host_config
    && let Some(binds) = &host_config.binds
  {
    let new_binds = binds
      .iter()
      .filter(|bind| !is_secret_bind(bind))
      .cloned()
      .collect::<Vec<_>>();
    config.host_config = Some(HostConfig {
      binds: if new_binds.is_empty() {
        None
      } else {
        Some(new_binds)
      },
      ..host_config.clone()
    });
  }
  config
}

/// Remove nanocld auto-mounted secret binds from a cargo spec.
fn clean_cargo_binds(cargo: &CargoSpecPartial) -> CargoSpecPartial {
  CargoSpecPartial {
    container: clean_config_binds(&cargo.container),
    init_container: cargo.init_container.as_ref().map(clean_config_binds),
    ..cargo.clone()
  }
}

/// Remove nanocld auto-mounted secret binds from a job spec.
fn clean_job_binds(job: &JobPartial) -> JobPartial {
  JobPartial {
    containers: job.containers.iter().map(clean_config_binds).collect(),
    ..job.clone()
  }
}

pub async fn exec_backup(
  cli_conf: &CliConfig,
  opts: &BackupOpts,
) -> IoResult<()> {
  let dir_path = opts
    .output_dir
    .clone()
    .unwrap_or(std::env::current_dir()?.to_string_lossy().to_string());
  std::fs::create_dir_all(&dir_path)?;
  // Backup namespaces
  let namespaces = cli_conf.client.list_namespace(None).await?;
  for namespace in namespaces {
    let file_path = format!("{}/{}.yml", dir_path, namespace.name);
    let token = format!("namespace/{}", namespace.name);
    let pg_style = utils::progress::create_spinner_style(&token, "green");
    let pg = utils::progress::create_progress("(processing)", &pg_style);
    pg.set_message("(processing: cargoes)");
    let cargoes = cli_conf
      .client
      .list_cargo(Some(&GenericFilterNsp {
        namespace: Some(namespace.name.clone()),
        ..Default::default()
      }))
      .await?
      .iter()
      .map(|cargo| {
        let cargo: CargoSpecPartial = cargo.spec.clone().into();
        clean_cargo_binds(&cargo)
      })
      .collect::<Vec<CargoSpecPartial>>();
    pg.set_message("(processing: virtual machines)");
    let vms = cli_conf
      .client
      .list_vm(Some(&GenericFilterNsp {
        namespace: Some(namespace.name.clone()),
        ..Default::default()
      }))
      .await?
      .iter()
      .map(|vm| vm.spec.clone().into())
      .collect::<Vec<VmSpecPartial>>();
    pg.set_message(format!("(writing statefile: {}.yml)", namespace.name));
    let state_file = Statefile {
      metadata: None,
      api_version: cli_conf.client.version.clone(),
      sub_states: None,
      args: None,
      group: None,
      namespace: Some(namespace.name.clone()),
      secrets: None,
      resources: None,
      cargoes: Some(cargoes),
      virtual_machines: Some(vms),
      jobs: None,
    };
    let data = serde_yaml::to_string(&state_file).map_err(|err| {
      IoError::interrupted("Backup state", err.to_string().as_str())
    })?;
    pg.finish_with_message(format!("(backup: {}.yml)", namespace.name));
    if std::path::Path::new(&file_path).exists() && !opts.skip_confirm {
      utils::dialog::confirm("File already exist override ?")?;
    }
    std::fs::write(&file_path, data)?;
  }
  // Backup jobs
  let file_path = format!("{}/jobs.yml", dir_path);
  let pg_style = utils::progress::create_spinner_style("jobs", "green");
  let pg = utils::progress::create_progress("(processing)", &pg_style);
  let jobs = cli_conf
    .client
    .list_job(None)
    .await?
    .iter()
    .map(|job| {
      let job: JobPartial = job.spec.clone().into();
      clean_job_binds(&job)
    })
    .collect::<Vec<JobPartial>>();
  if std::path::Path::new(&file_path).exists() && !opts.skip_confirm {
    utils::dialog::confirm("File already exist override ?")?;
  }
  let state_file = Statefile {
    metadata: None,
    api_version: cli_conf.client.version.clone(),
    sub_states: None,
    args: None,
    group: None,
    namespace: None,
    secrets: None,
    resources: None,
    cargoes: None,
    virtual_machines: None,
    jobs: Some(jobs),
  };
  let data = serde_yaml::to_string(&state_file).map_err(|err| {
    IoError::interrupted("Backup state", err.to_string().as_str())
  })?;
  std::fs::write(&file_path, data)?;
  pg.finish_with_message("(backup: jobs.yml)");
  // Backup secrets
  let file_path = format!("{}/secrets.yml", dir_path);
  let pg_style = utils::progress::create_spinner_style("secrets", "green");
  let pg = utils::progress::create_progress("(processing)", &pg_style);
  let secrets = cli_conf
    .client
    .list_secret(None)
    .await?
    .into_iter()
    .map(|secret| secret.into())
    .collect::<Vec<SecretPartial>>();
  if std::path::Path::new(&file_path).exists() && !opts.skip_confirm {
    utils::dialog::confirm("File already exist override ?")?;
  }
  let state_file = Statefile {
    metadata: None,
    api_version: cli_conf.client.version.clone(),
    sub_states: None,
    args: None,
    group: None,
    namespace: None,
    secrets: Some(secrets),
    resources: None,
    cargoes: None,
    virtual_machines: None,
    jobs: None,
  };
  let data = serde_yaml::to_string(&state_file).map_err(|err| {
    IoError::interrupted("Backup state", err.to_string().as_str())
  })?;
  std::fs::write(&file_path, data)?;
  pg.finish_with_message("(backup: secrets.yml)");
  // Backup resources
  let file_path = format!("{}/resources.yml", dir_path);
  let pg_style = utils::progress::create_spinner_style("resources", "green");
  let pg = utils::progress::create_progress("(processing)", &pg_style);
  let resources = cli_conf
    .client
    .list_resource(None)
    .await?
    .iter()
    .map(|resource| resource.clone().into())
    .collect::<Vec<ResourcePartial>>();
  if std::path::Path::new(&file_path).exists() && !opts.skip_confirm {
    utils::dialog::confirm("File already exist override ?")?;
  }
  let state_file = Statefile {
    metadata: None,
    api_version: cli_conf.client.version.clone(),
    sub_states: None,
    args: None,
    group: None,
    namespace: None,
    secrets: None,
    resources: Some(resources),
    cargoes: None,
    virtual_machines: None,
    jobs: None,
  };
  let data = serde_yaml::to_string(&state_file).map_err(|err| {
    IoError::interrupted("Backup state", err.to_string().as_str())
  })?;
  std::fs::write(&file_path, data)?;
  pg.finish_with_message("(backup: resources.yml)");
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_is_secret_bind() {
    assert!(is_secret_bind(
      "/var/lib/nanocl/secrets/cargo/test.nsp:/opt/nanocl.io/secrets"
    ));
    assert!(is_secret_bind(
      "/var/lib/nanocl/secrets/cargo/test.nsp:/opt/nanocl.io/secrets:ro"
    ));
    assert!(!is_secret_bind("/host/path:/container/path"));
    assert!(!is_secret_bind("/host/path:/opt/nanocl.io/secrets/sub"));
  }

  #[test]
  fn test_clean_config_binds_removes_secret_bind() {
    let config = Config {
      host_config: Some(HostConfig {
        binds: Some(vec![
          "/host/data:/data".to_owned(),
          "/var/lib/nanocl/secrets/cargo/test.nsp:/opt/nanocl.io/secrets"
            .to_owned(),
        ]),
        ..Default::default()
      }),
      ..Default::default()
    };
    let cleaned = clean_config_binds(&config);
    let binds = cleaned.host_config.unwrap().binds.unwrap();
    assert_eq!(binds, vec!["/host/data:/data".to_owned()]);
  }

  #[test]
  fn test_clean_config_binds_sets_empty_to_none() {
    let config = Config {
      host_config: Some(HostConfig {
        binds: Some(vec![
          "/var/lib/nanocl/secrets/cargo/test.nsp:/opt/nanocl.io/secrets"
            .to_owned(),
        ]),
        ..Default::default()
      }),
      ..Default::default()
    };
    let cleaned = clean_config_binds(&config);
    assert!(cleaned.host_config.unwrap().binds.is_none());
  }
}
