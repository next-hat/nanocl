use std::{collections::HashSet, future::Future, path::PathBuf};

use bollard_next::models::{ContainerCreateBody as Config, HostConfig};
use nanocl_error::io::{IoError, IoResult};
use nanocld_client::stubs::{
  cargo_spec::{CargoSpec, ContainerSpec},
  generic::{GenericFilter, GenericFilterNsp},
  job::JobPartial,
  namespace::NamespaceSummary,
  resource::ResourcePartial,
  secret::SecretPartial,
  statefile::Statefile,
  vm_spec::VmSpecPartial,
};

use crate::{
  config::CliConfig,
  models::BackupOpts,
  utils::{self, cargo::cargo_spec_from_revision},
};

/// Path inside containers where nanocld automatically mounts TLS secrets.
const NANOCL_SECRET_MOUNT: &str = "/opt/nanocl.io/secrets";

/// Check if a bind mount is the nanocld auto-mounted secret folder.
fn is_secret_bind(bind: &str) -> bool {
  bind.split(':').nth(1) == Some(NANOCL_SECRET_MOUNT)
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

fn clean_container_binds(container: &ContainerSpec) -> ContainerSpec {
  ContainerSpec {
    container_config: clean_config_binds(&container.container_config),
    ..container.clone()
  }
}

/// Remove nanocld auto-mounted secret binds from every Cargo container.
fn clean_cargo_binds(cargo: &CargoSpec) -> CargoSpec {
  CargoSpec {
    init_containers: cargo
      .init_containers
      .iter()
      .map(clean_container_binds)
      .collect(),
    containers: cargo.containers.iter().map(clean_container_binds).collect(),
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

/// Keep namespace documents separate from the three global exports. Encode the
/// name as one path component, even when an API namespace contains separators.
fn namespace_backup_paths(
  namespaces: &[NamespaceSummary],
) -> IoResult<Vec<PathBuf>> {
  let mut seen = HashSet::new();
  namespaces
    .iter()
    .map(|namespace| {
      let name: String =
        url::form_urlencoded::byte_serialize(namespace.name.as_bytes())
          .collect();
      let path = PathBuf::from("namespaces").join(format!("{name}.yml"));
      // Encoded names are ASCII. Reject aliases on case-insensitive filesystems.
      if !seen.insert(name.to_ascii_lowercase()) {
        return Err(IoError::invalid_data(
          "Backup",
          "duplicate namespace export path",
        ));
      }
      Ok(path)
    })
    .collect()
}

/// Use the existing offset API with a unique ordering key. This enumerates a
/// quiescent collection completely; it does not create a concurrent snapshot.
async fn collect_pages<T, F, Fut>(
  order_by: &str,
  mut fetch: F,
) -> IoResult<Vec<T>>
where
  F: FnMut(GenericFilter) -> Fut,
  Fut: Future<Output = IoResult<Vec<T>>>,
{
  const PAGE_SIZE: usize = 100;
  let mut items = Vec::new();
  loop {
    let page = fetch(GenericFilter {
      limit: Some(PAGE_SIZE),
      offset: Some(items.len()),
      order_by: Some(vec![order_by.to_owned()]),
      ..Default::default()
    })
    .await?;
    let exhausted = page.len() < PAGE_SIZE;
    items.extend(page);
    if exhausted {
      return Ok(items);
    }
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
  let namespaces = collect_pages("name asc", |filter| async move {
    Ok(cli_conf.client.list_namespace(Some(&filter)).await?)
  })
  .await?;
  let paths = namespace_backup_paths(&namespaces)?;
  std::fs::create_dir_all(std::path::Path::new(&dir_path).join("namespaces"))?;
  for (namespace, relative_path) in namespaces.into_iter().zip(paths) {
    let file_path = std::path::Path::new(&dir_path).join(&relative_path);
    let token = format!("namespace/{}", namespace.name);
    let pg_style = utils::progress::create_spinner_style(&token, "green");
    let pg = utils::progress::create_progress("(processing)", &pg_style);
    pg.set_message("(processing: cargoes)");
    let cargoes = collect_pages("key asc", |filter| {
      let query = GenericFilterNsp {
        namespace: Some(namespace.name.clone()),
        filter: Some(filter),
      };
      async move { Ok(cli_conf.client.list_cargo(Some(&query)).await?) }
    })
    .await?
    .iter()
    .map(|cargo| {
      let cargo = cargo_spec_from_revision(&cargo.spec);
      clean_cargo_binds(&cargo)
    })
    .collect::<Vec<CargoSpec>>();
    pg.set_message("(processing: virtual machines)");
    let vms = collect_pages("key asc", |filter| {
      let query = GenericFilterNsp {
        namespace: Some(namespace.name.clone()),
        filter: Some(filter),
      };
      async move { Ok(cli_conf.client.list_vm(Some(&query)).await?) }
    })
    .await?
    .iter()
    .map(|vm| vm.spec.clone().into())
    .collect::<Vec<VmSpecPartial>>();
    pg.set_message(format!("(writing statefile: {})", relative_path.display()));
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
    pg.finish_with_message(format!("(backup: {})", relative_path.display()));
    if std::path::Path::new(&file_path).exists() && !opts.skip_confirm {
      utils::dialog::confirm("File already exist override ?")?;
    }
    std::fs::write(&file_path, data)?;
  }
  // Backup jobs
  let file_path = format!("{}/jobs.yml", dir_path);
  let pg_style = utils::progress::create_spinner_style("jobs", "green");
  let pg = utils::progress::create_progress("(processing)", &pg_style);
  let jobs = collect_pages("key asc", |filter| async move {
    Ok(cli_conf.client.list_job(Some(&filter)).await?)
  })
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
  let secrets = collect_pages("key asc", |filter| async move {
    Ok(cli_conf.client.list_secret(Some(&filter)).await?)
  })
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
  let resources = collect_pages("key asc", |filter| async move {
    Ok(cli_conf.client.list_resource(Some(&filter)).await?)
  })
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
  fn backup_namespace_paths_are_disjoint_and_encoded() {
    let namespaces =
      ["jobs", "secrets", "resources", "../jobs", "a/b", "a%2Fb"].map(|name| {
        NamespaceSummary {
          name: name.into(),
          cargoes: 1,
          instances: 1,
          created_at: chrono::DateTime::UNIX_EPOCH.naive_utc(),
        }
      });
    let paths = namespace_backup_paths(&namespaces).unwrap();
    assert_eq!(paths[0], PathBuf::from("namespaces/jobs.yml"));
    for path in &paths {
      assert_eq!(path.parent().unwrap(), std::path::Path::new("namespaces"));
      for global in ["jobs.yml", "secrets.yml", "resources.yml"] {
        assert_ne!(path, &PathBuf::from(global));
      }
    }
    assert_eq!(paths.iter().collect::<HashSet<_>>().len(), namespaces.len());
    assert!(
      namespace_backup_paths(&[namespaces[0].clone(), namespaces[0].clone()])
        .is_err()
    );
  }

  #[test]
  fn backup_namespace_paths_reject_case_insensitive_collisions() {
    let namespaces = ["Prod", "prod"].map(|name| NamespaceSummary {
      name: name.into(),
      cargoes: 0,
      instances: 0,
      created_at: chrono::DateTime::UNIX_EPOCH.naive_utc(),
    });
    let error = namespace_backup_paths(&namespaces).unwrap_err();
    assert_eq!(error.inner.kind(), std::io::ErrorKind::InvalidData);
    assert!(
      error
        .inner
        .to_string()
        .contains("duplicate namespace export path")
    );
  }

  #[test]
  fn backup_pagination_collects_every_page() {
    for total in [0, 1, 100, 101, 201] {
      let mut offsets = Vec::new();
      let items =
        futures::executor::block_on(collect_pages("key asc", |filter| {
          assert_eq!(filter.limit, Some(100));
          assert_eq!(filter.order_by, Some(vec!["key asc".into()]));
          let offset = filter.offset.unwrap();
          offsets.push(offset);
          std::future::ready(Ok(
            (offset..total.min(offset + 100)).collect::<Vec<_>>(),
          ))
        }))
        .unwrap();
      assert_eq!(items, (0..total).collect::<Vec<_>>());
      assert_eq!(
        offsets,
        (0..=total / 100).map(|page| page * 100).collect::<Vec<_>>()
      );
    }
  }

  #[test]
  fn backup_pagination_propagates_later_page_failure() {
    let result =
      futures::executor::block_on(collect_pages("name asc", |filter| {
        assert_eq!(filter.order_by, Some(vec!["name asc".into()]));
        std::future::ready(if filter.offset == Some(0) {
          Ok((0..100).collect::<Vec<_>>())
        } else {
          Err(IoError::other("Backup fixture", "second page unavailable"))
        })
      }));
    assert!(
      result
        .unwrap_err()
        .inner
        .to_string()
        .contains("second page unavailable")
    );
  }

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

#[cfg(test)]
mod export_tests {
  use std::sync::{Arc, Mutex};

  use nanocld_client::{
    ConnectOpts, NanocldClient,
    stubs::{
      cargo::CargoSummary,
      cargo_spec::CargoSpecRevision,
      job::{JobSpec, JobSummary},
      resource::{Resource, ResourceSpec},
      secret::Secret,
      vm::VmSummary,
      vm_spec::VmSpec,
    },
  };
  use ntex::web;

  use super::*;
  use crate::{config::UserConfig, models::Context};

  struct ExportFixture {
    failing_collection: Option<String>,
    requests: Mutex<Vec<(String, usize)>>,
  }

  fn namespace_names() -> Vec<String> {
    let mut names = vec![
      "jobs".into(),
      "resources".into(),
      "secrets".into(),
      "zz-last".into(),
    ];
    names.extend((0..97).map(|index| format!("namespace-{index:03}")));
    names.sort();
    names
  }

  // HTTP/filesystem regression: compile locally, execute in CI.
  async fn serve_collection(request: web::HttpRequest) -> web::HttpResponse {
    let collection = request.path().rsplit('/').next().unwrap();
    let query: std::collections::HashMap<_, _> =
      url::form_urlencoded::parse(request.query_string().as_bytes())
        .into_owned()
        .collect();
    let filter: GenericFilter = serde_json::from_str(&query["filter"]).unwrap();
    assert_eq!(filter.limit, Some(100));
    assert_eq!(
      filter.order_by,
      Some(vec![format!(
        "{} asc",
        if collection == "namespaces" {
          "name"
        } else {
          "key"
        }
      )])
    );
    let offset = filter.offset.unwrap();
    let fixture = request.app_state::<Arc<ExportFixture>>().unwrap();
    fixture
      .requests
      .lock()
      .unwrap()
      .push((collection.to_owned(), offset));
    if fixture.failing_collection.as_deref() == Some(collection)
      && offset == 100
    {
      return web::HttpResponse::InternalServerError().json(&serde_json::json!({
        "msg": format!("later page failed for {collection} at offset {offset}"),
      }));
    }
    let time = chrono::DateTime::UNIX_EPOCH.naive_utc();
    if collection == "namespaces" {
      let items: Vec<_> = namespace_names()
        .into_iter()
        .skip(offset)
        .take(100)
        .map(|name| NamespaceSummary {
          name,
          cargoes: 101,
          instances: 0,
          created_at: time,
        })
        .collect();
      return web::HttpResponse::Ok().json(&items);
    }
    let namespace = if matches!(collection, "cargoes" | "vms") {
      let namespace = query
        .get("namespace")
        .expect("workload request must retain its namespace");
      assert!(namespace_names().contains(namespace));
      if !["jobs", "resources", "secrets", "zz-last"]
        .contains(&namespace.as_str())
      {
        return web::HttpResponse::Ok().json(&Vec::<serde_json::Value>::new());
      }
      namespace.as_str()
    } else {
      ""
    };
    let items: Vec<_> = (offset..101.min(offset + 100))
      .map(|index| {
        let name = format!("item-{index:03}");
        match collection {
          "cargoes" => serde_json::to_value(CargoSummary {
            namespace_name: namespace.into(),
            status: Default::default(),
            created_at: time,
            instance_total: 0,
            instance_running: 0,
            spec: CargoSpecRevision {
              name: name.clone(),
              cargo_key: format!("{namespace}.{name}"),
              secrets: vec!["item-100".into()],
              ..Default::default()
            },
          })
          .unwrap(),
          "vms" => serde_json::to_value(VmSummary {
            namespace_name: namespace.into(),
            status: Default::default(),
            created_at: time,
            instance_total: 0,
            instance_running: 0,
            spec: VmSpec {
              name: name.clone(),
              vm_key: format!("{namespace}.{name}"),
              ..Default::default()
            },
          })
          .unwrap(),
          "jobs" => serde_json::to_value(JobSummary {
            incarnation_key: uuid::Uuid::nil(),
            spec_key: uuid::Uuid::nil(),
            current_run_key: None,
            created_at: time,
            updated_at: time,
            status: Default::default(),
            instance_total: 0,
            instance_success: 0,
            instance_running: 0,
            instance_failed: 0,
            spec: JobSpec {
              name: name.clone(),
              job_key: name,
              ..Default::default()
            },
          })
          .unwrap(),
          "secrets" => serde_json::to_value(Secret {
            name,
            created_at: time,
            updated_at: time,
            kind: "Generic".into(),
            immutable: false,
            metadata: None,
            data: serde_json::json!({"fixture": true}),
          })
          .unwrap(),
          "resources" => serde_json::to_value(Resource {
            kind: "Fixture".into(),
            created_at: time,
            spec: ResourceSpec {
              key: uuid::Uuid::nil(),
              version: "v0.19".into(),
              created_at: time,
              resource_key: name,
              data: serde_json::json!({"fixture": true}),
              metadata: None,
            },
          })
          .unwrap(),
          _ => panic!("unexpected collection {collection}"),
        }
      })
      .collect();
    web::HttpResponse::Ok().json(&items)
  }

  fn fixture(
    failing_collection: Option<&str>,
  ) -> (CliConfig, web::test::TestServer, Arc<ExportFixture>) {
    let fixture = Arc::new(ExportFixture {
      failing_collection: failing_collection.map(str::to_owned),
      requests: Mutex::new(Vec::new()),
    });
    let server_fixture = fixture.clone();
    let server = web::test::server(async move |_| {
      web::App::new()
        .with_config(web::WebAppConfig::new().set_state(server_fixture.clone()))
        .service(
          web::resource("/{version}/{collection}")
            .route(web::get().to(serve_collection)),
        )
    });
    let host = server.url("").trim_end_matches('/').to_owned();
    let config = CliConfig {
      client: NanocldClient::connect_to(&ConnectOpts {
        url: host.clone(),
        ..Default::default()
      })
      .unwrap(),
      host,
      context: Context::default(),
      user_config: UserConfig::default(),
    };
    (config, server, fixture)
  }

  #[ntex::test]
  async fn backup_http_exports_all_collections_and_colliding_namespace_names() {
    let (config, _server, _fixture) = fixture(None);
    for skip_confirm in [false, true] {
      let dir = std::env::temp_dir()
        .join(format!("nanocl-backup-{}", uuid::Uuid::new_v4()));
      exec_backup(
        &config,
        &BackupOpts {
          output_dir: Some(dir.to_string_lossy().into()),
          skip_confirm,
        },
      )
      .await
      .unwrap();
      let read = |path: &str| -> Statefile {
        serde_yaml::from_str(&std::fs::read_to_string(dir.join(path)).unwrap())
          .unwrap()
      };
      assert_eq!(
        std::fs::read_dir(dir.join("namespaces")).unwrap().count(),
        101
      );
      for namespace in ["jobs", "resources", "secrets", "zz-last"] {
        let state = read(&format!("namespaces/{namespace}.yml"));
        assert_eq!(state.namespace.as_deref(), Some(namespace));
        let cargoes = state.cargoes.unwrap();
        assert_eq!(cargoes.len(), 101);
        assert_eq!(cargoes.last().unwrap().name, "item-100");
        assert_eq!(cargoes[0].secrets, vec!["item-100"]);
        assert_eq!(state.virtual_machines.unwrap().len(), 101);
      }
      let jobs = read("jobs.yml");
      assert!(jobs.namespace.is_none());
      assert_eq!(jobs.jobs.unwrap().len(), 101);
      let secrets = read("secrets.yml").secrets.unwrap();
      assert_eq!(secrets.len(), 101);
      assert_eq!(secrets.last().unwrap().name, "item-100");
      assert_eq!(read("resources.yml").resources.unwrap().len(), 101);
      std::fs::remove_dir_all(dir).unwrap();
    }
  }

  #[ntex::test]
  async fn backup_http_rejects_later_page_failure_in_every_collection() {
    for collection in [
      "namespaces",
      "cargoes",
      "vms",
      "jobs",
      "secrets",
      "resources",
    ] {
      let (config, _server, fixture) = fixture(Some(collection));
      let dir = std::env::temp_dir()
        .join(format!("nanocl-backup-failure-{}", uuid::Uuid::new_v4()));
      let result = exec_backup(
        &config,
        &BackupOpts {
          output_dir: Some(dir.to_string_lossy().into()),
          skip_confirm: true,
        },
      )
      .await;
      let error = result
        .expect_err("later-page failure must prevent backup success")
        .to_string();
      assert!(
        error.contains(&format!(
          "later page failed for {collection} at offset 100"
        )),
        "wrong failure for {collection}: {error}",
      );
      let offsets: Vec<_> = fixture
        .requests
        .lock()
        .unwrap()
        .iter()
        .filter(|(name, _)| name == collection)
        .map(|(_, offset)| *offset)
        .collect();
      assert_eq!(
        offsets,
        vec![0, 100],
        "{collection} must reach the injected second page"
      );
      std::fs::remove_dir_all(dir).unwrap();
    }
  }
}
