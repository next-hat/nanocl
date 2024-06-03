use nanocl_error::io::{IoError, IoResult};
use nanocld_client::stubs::{
  cargo_spec::CargoSpecPartial, generic::GenericFilterNsp, job::JobPartial,
  resource::ResourcePartial, secret::SecretPartial, statefile::Statefile,
  vm_spec::VmSpecPartial,
};

use crate::{utils, config::CliConfig, models::BackupOpts};

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
      .map(|cargo| cargo.spec.clone().into())
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
    .map(|job| job.spec.clone().into())
    .collect::<Vec<JobPartial>>();
  if std::path::Path::new(&file_path).exists() && !opts.skip_confirm {
    utils::dialog::confirm("File already exist override ?")?;
  }
  let state_file = Statefile {
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
