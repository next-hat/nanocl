use chrono::TimeZone;
use clap::{Parser, Subcommand};
use tabled::Tabled;

use nanocld_client::stubs::{
  cargo::CargoSummary,
  cargo_spec::{CargoSpec, Config, ContainerSpec, HostConfig},
};

#[cfg(test)]
use nanocld_client::stubs::cargo_spec::CargoSpecRevision;

use super::{
  GenericInspectOpts, GenericRemoveForceOpts, GenericRemoveOpts,
  GenericStartOpts, GenericStopOpts, NamespacedListOpts,
};

/// `nanocl cargo create` available options
#[derive(Clone, Parser)]
pub struct CargoCreateOpts {
  /// Namespace for the new cargo; defaults to global
  #[clap(short, long)]
  pub namespace: Option<String>,
  /// Name of the cargo
  pub name: String,
  /// Image of the cargo
  pub image: String,
  /// Volumes of the cargo
  #[clap(short, long = "volume")]
  pub volumes: Option<Vec<String>>,
  /// Environment variables of the cargo
  #[clap(short, long = "env")]
  pub(crate) env: Option<Vec<String>>,
}

fn single_container_cargo_spec(
  name: String,
  image: String,
  env: Option<Vec<String>>,
  volumes: Option<Vec<String>>,
  command: Option<Vec<String>>,
) -> CargoSpec {
  CargoSpec {
    name: name.clone(),
    containers: vec![ContainerSpec {
      name,
      essential: true,
      secrets: Vec::new(),
      image_pull_secret: None,
      image_pull_policy: Default::default(),
      container_config: Config {
        image: Some(image),
        env,
        cmd: command,
        host_config: Some(HostConfig {
          binds: volumes,
          ..Default::default()
        }),
        ..Default::default()
      },
    }],
    ..Default::default()
  }
}

/// Convert CargoCreateOpts to CargoSpec
impl From<CargoCreateOpts> for CargoSpec {
  fn from(val: CargoCreateOpts) -> Self {
    single_container_cargo_spec(val.name, val.image, val.env, val.volumes, None)
  }
}

/// `nanocl cargo run` available options
#[derive(Clone, Parser)]
pub struct CargoRunOpts {
  /// Namespace for the new cargo; defaults to global
  #[clap(short, long)]
  pub namespace: Option<String>,
  /// Name of the cargo
  pub name: String,
  /// Image of the cargo
  pub image: String,
  /// Volumes of the cargo
  #[clap(short, long = "volume")]
  pub volumes: Option<Vec<String>>,
  /// Environment variables of the cargo
  #[clap(short, long = "env")]
  pub env: Option<Vec<String>>,
  /// Command to execute
  pub command: Vec<String>,
}

/// Convert CargoRunOpts to CargoSpec
impl From<CargoRunOpts> for CargoSpec {
  fn from(val: CargoRunOpts) -> Self {
    single_container_cargo_spec(
      val.name,
      val.image,
      val.env,
      val.volumes,
      Some(val.command),
    )
  }
}

/// `nanocl cargo restart` available options
#[derive(Clone, Parser)]
pub struct CargoRestartOpts {
  // Canonical keys of cargoes to restart
  pub keys: Vec<String>,
}

/// `nanocl cargo patch` available options
#[derive(Clone, Parser)]
pub struct CargoPatchOpts {
  /// Canonical key of the cargo to update
  pub(crate) key: String,
  /// Name of the application container to update
  #[clap(long)]
  pub(crate) container: Option<String>,
  /// New image of cargo
  #[clap(short, long = "image")]
  pub(crate) image: Option<String>,
  /// New environment variables of cargo
  #[clap(short, long = "env")]
  pub(crate) env: Option<Vec<String>>,
  /// New volumes of cargo
  #[clap(short, long = "volume")]
  pub(crate) volumes: Option<Vec<String>>,
}

/// `nanocl cargo history` available options
#[derive(Clone, Parser)]
pub struct CargoHistoryOpts {
  /// Canonical key of the cargo whose history to browse
  pub key: String,
}

/// `nanocl cargo revert` available options
#[derive(Clone, Parser)]
pub struct CargoRevertOpts {
  /// Canonical key of the cargo to revert
  pub key: String,
  /// Revert to a specific historic
  pub history_id: String,
}

/// `nanocl cargo logs` available options
#[derive(Clone, Parser)]
pub struct CargoLogsOpts {
  /// Canonical key of the cargo whose logs to show
  pub key: String,
  /// Only include logs since unix timestamp
  #[clap(short = 's')]
  pub since: Option<i64>,
  /// Only include logs until unix timestamp
  #[clap(short = 'u')]
  pub until: Option<i64>,
  /// If integer only return last n logs, if "all" returns all logs
  #[clap(short = 't')]
  pub tail: Option<String>,
  /// Bool, if set include timestamp to ever log line
  #[clap(long = "timestamps")]
  pub timestamps: bool,
  /// Bool, if set open the log as stream
  #[clap(short = 'f')]
  pub follow: bool,
}

/// `nanocl cargo stats` available options
#[derive(Clone, Parser)]
pub struct CargoStatsOpts {
  /// Canonical keys of cargoes whose stats to show
  pub keys: Vec<String>,
  /// Disable streaming stats and only pull the first result
  #[clap(long)]
  pub no_stream: bool,
  // TODO: Show all containers (default shows just running)
  // pub all: bool,
}

/// `nanocl cargo` available commands
#[derive(Clone, Subcommand)]
pub enum CargoCommand {
  /// List existing cargo
  #[clap(alias("ls"))]
  List(NamespacedListOpts),
  /// Create a new cargo
  Create(CargoCreateOpts),
  /// Start cargoes by canonical keys
  Start(GenericStartOpts),
  /// Stop cargoes by canonical keys
  Stop(GenericStopOpts),
  /// Restart cargoes by canonical keys
  Restart(CargoRestartOpts),
  /// Remove cargoes by canonical keys
  #[clap(alias("rm"))]
  Remove(GenericRemoveOpts<GenericRemoveForceOpts>),
  /// Inspect a cargo by its canonical key
  Inspect(GenericInspectOpts),
  /// Update a cargo by its canonical key
  Patch(CargoPatchOpts),
  /// List cargo history
  History(CargoHistoryOpts),
  /// Revert cargo to a specific history
  Revert(CargoRevertOpts),
  /// Show logs
  Logs(CargoLogsOpts),
  /// Run a cargo
  Run(CargoRunOpts),
  /// Show stats of cargo
  Stats(CargoStatsOpts),
}

/// `nanocl cargo` available arguments
#[derive(Clone, Parser)]
pub struct CargoArg {
  #[clap(subcommand)]
  pub command: CargoCommand,
}

/// A row of the cargo table
#[derive(Tabled)]
#[tabled(rename_all = "UPPERCASE")]
pub struct CargoRow {
  /// Canonical key of the cargo
  pub(crate) key: String,
  /// Named application containers and their images
  pub(crate) containers: String,
  /// Status of the cargo
  pub(crate) status: String,
  /// Number of running instances
  pub(crate) instances: String,
  /// Spec version of the cargo
  pub(crate) version: String,
  /// When the cargo was created
  #[tabled(rename = "CREATED AT")]
  pub(crate) created_at: String,
  /// When the cargo was last updated
  #[tabled(rename = "UPDATED AT")]
  pub(crate) updated_at: String,
}

/// Convert CargoSummary to CargoRow
impl From<CargoSummary> for CargoRow {
  fn from(cargo: CargoSummary) -> Self {
    let binding = chrono::Local::now();
    let tz = binding.offset();
    // Convert the created_at and updated_at to the current timezone
    let created_at = tz
      .timestamp_opt(cargo.created_at.and_utc().timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    let updated_at = tz
      .timestamp_opt(cargo.spec.created_at.and_utc().timestamp(), 0)
      .unwrap()
      .format("%Y-%m-%d %H:%M:%S");
    Self {
      key: cargo.spec.cargo_key,
      containers: cargo
        .spec
        .containers
        .iter()
        .map(|container| {
          format!(
            "{}={}",
            container.name,
            container
              .container_config
              .image
              .as_deref()
              .unwrap_or_default()
          )
        })
        .collect::<Vec<_>>()
        .join(", "),
      version: cargo.spec.version,
      status: format!(
        "{}/{} ({})",
        cargo.status.actual, cargo.status.wanted, cargo.status.health
      ),
      instances: format!("{}/{}", cargo.instance_running, cargo.instance_total),
      created_at: format!("{created_at}"),
      updated_at: format!("{updated_at}"),
    }
  }
}

#[cfg(test)]
mod tests {
  use std::path::{Path, PathBuf};

  use clap::Parser;
  use regex::Regex;

  use crate::utils::cargo::build_cargo_patch;

  use super::*;

  fn container(name: &str, image: &str) -> ContainerSpec {
    ContainerSpec {
      name: name.to_owned(),
      essential: true,
      secrets: Vec::new(),
      image_pull_secret: None,
      image_pull_policy: Default::default(),
      container_config: Config {
        image: Some(image.to_owned()),
        ..Default::default()
      },
    }
  }

  fn collect_rust_sources(path: &Path, sources: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(path).unwrap() {
      let path = entry.unwrap().path();
      if path.is_dir() {
        collect_rust_sources(&path, sources);
      } else if path.extension().is_some_and(|extension| extension == "rs") {
        sources.push(path);
      }
    }
  }

  #[test]
  fn cargo_cli_sources_have_no_legacy_types_or_implicit_selector() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut sources = vec![manifest.join("build.rs")];
    collect_rust_sources(&manifest.join("src"), &mut sources);

    let legacy_types = [
      ["CargoSpec", "Partial"].concat(),
      ["CargoSpec", "Update"].concat(),
    ];
    let singular_spec = Regex::new(r"\bspec\s*\.\s*container\b").unwrap();
    let implicit_selectors = [
      Regex::new(r"\bcontainers\s*\.\s*first\s*\(").unwrap(),
      Regex::new(r"\bcontainers\s*\.\s*get\s*\(\s*0\s*\)").unwrap(),
      Regex::new(r"\bcontainers\s*\[\s*0\s*\]").unwrap(),
    ];

    for path in sources {
      let source = std::fs::read_to_string(&path).unwrap();
      for legacy_type in &legacy_types {
        assert!(
          !source.contains(legacy_type),
          "legacy Cargo type remains in {}",
          path.display()
        );
      }
      assert!(
        !singular_spec.is_match(&source),
        "singular Cargo revision access remains in {}",
        path.display()
      );
      for selector in &implicit_selectors {
        assert!(
          !selector.is_match(&source),
          "implicit application-container selection remains in {}",
          path.display()
        );
      }
    }
  }

  #[test]
  fn create_builds_a_named_single_container_spec() {
    let spec: CargoSpec = CargoCreateOpts {
      namespace: Some("system".to_owned()),
      name: "api".to_owned(),
      image: "example/api:1".to_owned(),
      volumes: Some(vec!["./data:/data".to_owned()]),
      env: Some(vec!["MODE=prod".to_owned()]),
    }
    .into();

    assert_eq!(spec.name, "api");
    let [container] = spec.containers.as_slice() else {
      panic!("create must produce exactly one application container");
    };
    assert_eq!(container.name, "api");
    assert_eq!(
      container.container_config.image.as_deref(),
      Some("example/api:1")
    );
    assert_eq!(
      container.container_config.env.as_deref(),
      Some(["MODE=prod".to_owned()].as_slice())
    );
    assert_eq!(
      container
        .container_config
        .host_config
        .as_ref()
        .and_then(|host| host.binds.as_deref()),
      Some(["./data:/data".to_owned()].as_slice())
    );
  }

  #[test]
  fn run_builds_a_named_single_container_spec_with_command() {
    let spec: CargoSpec = CargoRunOpts {
      namespace: None,
      name: "worker".to_owned(),
      image: "example/worker:2".to_owned(),
      volumes: Some(vec!["cache:/cache".to_owned()]),
      env: Some(vec!["QUEUE=critical".to_owned()]),
      command: vec!["sh".to_owned(), "-c".to_owned(), "work".to_owned()],
    }
    .into();

    assert_eq!(spec.name, "worker");
    let [container] = spec.containers.as_slice() else {
      panic!("run must produce exactly one application container");
    };
    assert_eq!(container.name, "worker");
    assert_eq!(
      container.container_config.cmd.as_deref(),
      Some(["sh".to_owned(), "-c".to_owned(), "work".to_owned()].as_slice())
    );
    assert_eq!(
      container.container_config.env.as_deref(),
      Some(["QUEUE=critical".to_owned()].as_slice())
    );
    assert_eq!(
      container
        .container_config
        .host_config
        .as_ref()
        .and_then(|host| host.binds.as_deref()),
      Some(["cache:/cache".to_owned()].as_slice())
    );
  }

  #[test]
  fn run_rejects_removed_rm_option() {
    assert!(
      CargoArg::try_parse_from(["cargo", "run", "worker", "alpine", "--rm"])
        .is_err()
    );
  }

  #[test]
  fn patch_implicitly_updates_the_only_application_container() {
    let current = vec![container("api", "example/api:1")];
    let patch = build_cargo_patch(
      &CargoPatchOpts {
        key: "global.api".to_owned(),
        container: None,
        image: Some("example/api:2".to_owned()),
        env: None,
        volumes: None,
      },
      &current,
    )
    .unwrap();

    let containers = patch.containers.unwrap();
    let [updated] = containers.as_slice() else {
      panic!("patch must replace the complete application list");
    };
    assert_eq!(updated.name, "api");
    assert_eq!(
      updated.container_config.image.as_deref(),
      Some("example/api:2")
    );
  }

  #[test]
  fn patch_requires_a_selector_for_multiple_application_containers() {
    let error = build_cargo_patch(
      &CargoPatchOpts {
        key: "global.stack".to_owned(),
        container: None,
        image: Some("example/api:2".to_owned()),
        env: None,
        volumes: None,
      },
      &[
        container("api", "example/api:1"),
        container("worker", "example/worker:1"),
      ],
    )
    .unwrap_err();

    assert_eq!(error.inner.kind(), std::io::ErrorKind::InvalidInput);
    assert!(error.inner.to_string().contains("--container is required"));
  }

  #[test]
  fn patch_rejects_an_unknown_named_container() {
    let error = build_cargo_patch(
      &CargoPatchOpts {
        key: "global.stack".to_owned(),
        container: Some("missing".to_owned()),
        image: Some("example/missing:2".to_owned()),
        env: None,
        volumes: None,
      },
      &[
        container("api", "example/api:1"),
        container("worker", "example/worker:1"),
      ],
    )
    .unwrap_err();

    assert_eq!(error.inner.kind(), std::io::ErrorKind::InvalidInput);
    assert!(
      error
        .inner
        .to_string()
        .contains("\"missing\" does not exist")
    );
  }

  #[test]
  fn patch_replaces_the_full_list_and_only_changes_the_named_container() {
    let api = container("api", "example/api:1");
    let mut worker = container("worker", "example/worker:1");
    worker.container_config.env = Some(vec!["KEEP=yes".to_owned()]);
    worker.container_config.host_config = Some(HostConfig {
      privileged: Some(true),
      binds: Some(vec!["old:/old".to_owned()]),
      ..Default::default()
    });
    let patch = build_cargo_patch(
      &CargoPatchOpts {
        key: "global.stack".to_owned(),
        container: Some("worker".to_owned()),
        image: None,
        env: Some(vec!["MODE=batch".to_owned()]),
        volumes: Some(vec!["new:/new".to_owned()]),
      },
      &[api.clone(), worker],
    )
    .unwrap();

    let containers = patch.containers.unwrap();
    let [unchanged, updated] = containers.as_slice() else {
      panic!("patch must preserve the complete application list");
    };
    assert_eq!(unchanged, &api);
    assert_eq!(updated.name, "worker");
    assert_eq!(
      updated.container_config.image.as_deref(),
      Some("example/worker:1")
    );
    assert_eq!(
      updated.container_config.env.as_deref(),
      Some(["MODE=batch".to_owned()].as_slice())
    );
    let host = updated.container_config.host_config.as_ref().unwrap();
    assert_eq!(host.privileged, Some(true));
    assert_eq!(
      host.binds.as_deref(),
      Some(["new:/new".to_owned()].as_slice())
    );
  }

  #[test]
  fn list_row_renders_every_application_container_in_declaration_order() {
    let row = CargoRow::from(CargoSummary {
      namespace_name: "global".to_owned(),
      status: Default::default(),
      created_at: Default::default(),
      instance_total: 1,
      instance_running: 1,
      spec: CargoSpecRevision {
        cargo_key: "global.stack".to_owned(),
        version: "v1".to_owned(),
        containers: vec![
          container("api", "example/api:1"),
          container("worker", "example/worker:2"),
        ],
        ..Default::default()
      },
    });

    assert_eq!(row.containers, "api=example/api:1, worker=example/worker:2");
    assert!(tabled::Table::new([row]).to_string().contains("CONTAINERS"));
  }

  #[test]
  fn namespace_is_only_available_for_collection_and_creation_commands() {
    let inspect =
      CargoArg::try_parse_from(["cargo", "inspect", "system.same"]).unwrap();
    assert!(matches!(inspect.command, CargoCommand::Inspect(_)));
    assert!(
      CargoArg::try_parse_from([
        "cargo",
        "--namespace",
        "system",
        "inspect",
        "system.same"
      ])
      .is_err()
    );

    let list =
      CargoArg::try_parse_from(["cargo", "list", "--namespace", "system"])
        .unwrap();
    let CargoCommand::List(list) = list.command else {
      panic!("expected list command");
    };
    assert_eq!(list.namespace.as_deref(), Some("system"));

    let create =
      CargoArg::try_parse_from(["cargo", "create", "new", "alpine"]).unwrap();
    let CargoCommand::Create(create) = create.command else {
      panic!("expected create command");
    };
    assert_eq!(create.namespace, None);
  }

  #[test]
  fn table_rows_distinguish_duplicate_local_names() {
    let table = tabled::Table::new([
      CargoRow {
        key: "global.same".to_owned(),
        containers: "same=alpine".to_owned(),
        status: "running".to_owned(),
        instances: "1/1".to_owned(),
        version: "v1".to_owned(),
        created_at: String::new(),
        updated_at: String::new(),
      },
      CargoRow {
        key: "system.same".to_owned(),
        containers: "same=alpine".to_owned(),
        status: "running".to_owned(),
        instances: "1/1".to_owned(),
        version: "v1".to_owned(),
        created_at: String::new(),
        updated_at: String::new(),
      },
    ])
    .to_string();
    assert!(table.contains("KEY"));
    assert!(table.contains("global.same"));
    assert!(table.contains("system.same"));
  }
}
