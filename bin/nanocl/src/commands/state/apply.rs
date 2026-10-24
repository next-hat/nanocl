use futures::{StreamExt, stream::FuturesUnordered};
use indicatif::{MultiProgress, ProgressBar};
use serde_json::Value;

use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocld_client::stubs::{
  cargo_spec::CargoSpec,
  generic::{GenericClause, GenericFilter, GenericFilterNsp},
  resource::{ResourcePartial, ResourceUpdate},
  secret::{SecretPartial, SecretUpdate},
  statefile::Statefile,
  system::{EventActorKind, NativeEventAction, ObjPsStatusKind},
  vm_spec::{VmSpecPartial, VmSpecUpdate},
};

use crate::{
  config::CliConfig,
  models::{StateApplyOpts, StateLogsOpts, StateOutput, StateRef},
  utils::{self, cargo::cargo_spec_from_revision},
};

use super::{
  ArgParseMode, get_nanocl_group, logs::state_logs, parse_build_args,
  parse_state_file_recurr, print_states, read_state_file, remove::state_remove,
  state_item_count,
};

fn insert_nanocl_group(
  metadata: &Option<serde_json::Value>,
  group: &str,
) -> serde_json::Value {
  match metadata {
    Some(metadata) => {
      let mut metadata = metadata.clone();
      metadata.as_object_mut().unwrap().insert(
        "io.nanocl.group".to_owned(),
        Value::String(group.to_owned()),
      );
      metadata
    }
    None => serde_json::json!({
      "io.nanocl.group": group,
    }),
  }
}

fn vm_spec_for_comparison(
  desired: &VmSpecPartial,
  current: &VmSpecPartial,
) -> IoResult<VmSpecPartial> {
  utils::state_diff::validate_vm_identity(
    desired.mac_address.as_deref(),
    current.mac_address.as_deref(),
  )?;
  // Resolve omitted values only for comparison. PATCH must retain omissions
  // so a concurrent change after this inspect is not overwritten.
  let mut desired = desired.clone();
  desired.mac_address = current.mac_address.clone();
  desired.cloud_init = desired.cloud_init.or(current.cloud_init);
  desired.cloud_init_user_data = desired
    .cloud_init_user_data
    .clone()
    .or_else(|| current.cloud_init_user_data.clone());
  desired.console = desired.console.or(current.console);
  if matches!(
    desired.cloud_init_user_data,
    Some(nanocld_client::stubs::vm_spec::VmCloudInitUserData::Reset)
  ) {
    desired.cloud_init_user_data = None;
  }
  Ok(desired)
}

async fn apply_matching_vm(
  client: &nanocld_client::NanocldClient,
  key: &str,
  actual: &ObjPsStatusKind,
  pg: &ProgressBar,
  summary: &ProgressBar,
  output: Option<&StateOutput>,
) -> IoResult<&'static str> {
  if *actual == ObjPsStatusKind::Start {
    return Ok("Unchanged");
  }
  if *actual == ObjPsStatusKind::Stop {
    let waiter = utils::process::wait_process_state(
      key,
      EventActorKind::Vm,
      vec![NativeEventAction::Start],
      client,
    )
    .await?;
    utils::progress::set_state_message(pg, summary, "Starting", output)?;
    client.start_process("vm", key).await?;
    waiter.await.map_err(|err| {
      IoError::interrupted("wait_process_state", &err.to_string())
    })??;
  }
  Ok("Running")
}

async fn state_apply(
  cli_conf: &CliConfig,
  opts: &StateApplyOpts,
  state_file: &StateRef<Statefile>,
  progress: &MultiProgress,
  summary: &ProgressBar,
  output: Option<&StateOutput>,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let namespace = state_file.data.namespace.as_deref().unwrap_or("global");
  let nanocl_group = &get_nanocl_group(state_file);
  if let Some(secrets) = &state_file.data.secrets {
    for secret in secrets.iter() {
      let mut secret = secret.to_owned();
      let token = format!("secret/{}", secret.name);
      utils::progress::run_state_step(
        progress,
        summary,
        &token,
        output,
        None,
        |pg| async move {
          let metadata = insert_nanocl_group(&secret.metadata, nanocl_group);
          secret.metadata = Some(metadata);
          utils::progress::set_state_message(&pg, summary, "Applying", output)?;
          match client.inspect_secret(&secret.name).await {
            Err(_) => {
              client.create_secret(&secret).await?;
              Ok("Created")
            }
            Ok(inspect) => {
              let cmp: SecretPartial = inspect.into();
              if cmp != secret {
                let update: SecretUpdate = secret.clone().into();
                client.patch_secret(&secret.name, &update).await?;
                Ok("Updated")
              } else {
                Ok("Unchanged")
              }
            }
          }
        },
      )
      .await?;
    }
  }

  if let Some(jobs) = &state_file.data.jobs {
    for job in jobs.iter() {
      let mut job = job.to_owned();
      let key = job.name.clone();
      let token = format!("job/{key}");
      utils::progress::run_state_step(
        progress,
        summary,
        &token,
        output,
        Some((client, &key, EventActorKind::Job)),
        |pg| async move {
          let metadata = insert_nanocl_group(&job.metadata, nanocl_group);
          job.metadata = Some(metadata);
          if client.inspect_job(&job.name).await.is_ok() {
            utils::progress::set_state_message(
              &pg, summary, "Clearing", output,
            )?;
            let waiter = utils::process::wait_process_state(
              &job.name,
              EventActorKind::Job,
              vec![NativeEventAction::Destroy],
              client,
            )
            .await?;
            client.delete_job(&job.name).await?;
            waiter.await.map_err(|err| {
              IoError::interrupted("wait_process_state", &err.to_string())
            })??;
            utils::progress::set_state_message(
              &pg, summary, "Cleared", output,
            )?;
          }
          utils::progress::set_state_message(&pg, summary, "Creating", output)?;
          client.create_job(&job).await?;
          let waiter = utils::process::wait_process_state(
            &job.name,
            EventActorKind::Job,
            vec![NativeEventAction::Start],
            client,
          )
          .await?;
          utils::progress::set_state_message(&pg, summary, "Starting", output)?;
          client.start_process("job", &job.name).await?;
          waiter.await.map_err(|err| {
            IoError::interrupted("wait_process_state", &err.to_string())
          })??;
          Ok("Running")
        },
      )
      .await?;
    }
  }

  if let Some(cargoes) = &state_file.data.cargoes {
    for cargo in cargoes.iter() {
      let mut cargo = cargo.to_owned();
      let key = utils::process::resource_key(&cargo.name, namespace)?;
      let token = format!("cargo/{key}");
      utils::progress::run_state_step(
        progress,
        summary,
        &token,
        output,
        Some((client, &key, EventActorKind::Cargo)),
        |pg| {
          let key = &key;
          async move {
            let metadata = insert_nanocl_group(&cargo.metadata, nanocl_group);
            cargo.metadata = Some(metadata);
            match client.inspect_cargo(key).await {
              Err(_) => {
                utils::progress::set_state_message(
                  &pg, summary, "Creating", output,
                )?;
                client.create_cargo(&cargo, Some(namespace)).await?;
                let waiter = utils::process::wait_process_state(
                  key,
                  EventActorKind::Cargo,
                  vec![NativeEventAction::Start],
                  client,
                )
                .await?;
                utils::progress::set_state_message(
                  &pg, summary, "Starting", output,
                )?;
                client.start_process("cargo", key).await?;
                waiter.await.map_err(|err| {
                  IoError::interrupted("wait_process_state", &err.to_string())
                })??;
              }
              Ok(inspect) => {
                let cmp = cargo_spec_from_revision(&inspect.spec);
                if (cmp != cargo) || opts.reload {
                  utils::progress::set_state_message(
                    &pg, summary, "Updating", output,
                  )?;
                  let waiter = utils::process::wait_process_state(
                    key,
                    EventActorKind::Cargo,
                    vec![NativeEventAction::Update],
                    client,
                  )
                  .await?;
                  client.put_cargo(key, &cargo).await?;
                  waiter.await.map_err(|err| {
                    IoError::interrupted("wait_process_state", &err.to_string())
                  })??;
                  utils::progress::set_state_message(
                    &pg, summary, "Updated", output,
                  )?;
                } else if inspect.status.actual == ObjPsStatusKind::Start {
                  return Ok("Unchanged");
                }
              }
            }
            Ok("Running")
          }
        },
      )
      .await?;
    }
  }

  if let Some(vms) = &state_file.data.virtual_machines {
    for vm in vms.iter() {
      let mut vm = vm.to_owned();
      let key = utils::process::resource_key(&vm.name, namespace)?;
      let token = format!("vm/{key}");
      utils::progress::run_state_step(
        progress,
        summary,
        &token,
        output,
        Some((client, &key, EventActorKind::Vm)),
        |pg| {
          let key = &key;
          async move {
            let metadata = insert_nanocl_group(&vm.metadata, nanocl_group);
            vm.metadata = Some(metadata);
            match client.inspect_vm(key).await {
              Err(_) => {
                utils::progress::set_state_message(
                  &pg, summary, "Creating", output,
                )?;
                let image_full_path =
                  utils::path::resolve_full_path(&vm.image)?;
                vm.image = image_full_path;
                client.create_vm(&vm, Some(namespace)).await?;
                let waiter = utils::process::wait_process_state(
                  key,
                  EventActorKind::Vm,
                  vec![NativeEventAction::Start],
                  client,
                )
                .await?;
                utils::progress::set_state_message(
                  &pg, summary, "Starting", output,
                )?;
                client.start_process("vm", key).await?;
                waiter.await.map_err(|err| {
                  IoError::interrupted("wait_process_state", &err.to_string())
                })??;
              }
              Ok(inspect) => {
                let cmp: VmSpecPartial = inspect.spec.into();
                let comparison = vm_spec_for_comparison(&vm, &cmp)?;
                if (cmp != comparison) || opts.reload {
                  let update: VmSpecUpdate = vm.clone().into();
                  utils::progress::set_state_message(
                    &pg, summary, "Updating", output,
                  )?;
                  let waiter = utils::process::wait_process_state(
                    key,
                    EventActorKind::Vm,
                    vec![NativeEventAction::Start],
                    client,
                  )
                  .await?;
                  client.patch_vm(key, &update).await?;
                  waiter.await.map_err(|err| {
                    IoError::interrupted("wait_process_state", &err.to_string())
                  })??;
                  utils::progress::set_state_message(
                    &pg, summary, "Updated", output,
                  )?;
                } else {
                  return apply_matching_vm(
                    client,
                    key,
                    &inspect.status.actual,
                    &pg,
                    summary,
                    output,
                  )
                  .await;
                }
              }
            }
            Ok("Running")
          }
        },
      )
      .await?;
    }
  }

  if let Some(resources) = &state_file.data.resources {
    for resource in resources.iter() {
      let mut resource = resource.to_owned();
      let token = format!("resource/{}", resource.name);
      utils::progress::run_state_step(
        progress,
        summary,
        &token,
        output,
        None,
        |pg| async move {
          let metadata = insert_nanocl_group(&resource.metadata, nanocl_group);
          resource.metadata = Some(metadata);
          utils::progress::set_state_message(&pg, summary, "Applying", output)?;
          match client.inspect_resource(&resource.name).await {
            Err(_) => {
              client.create_resource(&resource).await?;
              Ok("Created")
            }
            Ok(inspect) => {
              let cmp: ResourcePartial = inspect.into();
              if (cmp != resource) || opts.reload {
                let update: ResourceUpdate = resource.clone().into();
                client.put_resource(&resource.name, &update).await?;
                Ok("Updated")
              } else {
                Ok("Unchanged")
              }
            }
          }
        },
      )
      .await?;
    }
  }

  Ok(())
}

async fn remove_orphans(
  cli_conf: &CliConfig,
  state: &StateRef<Statefile>,
  json: bool,
) -> IoResult<()> {
  let filter = GenericFilter::new().r#where(
    "metadata",
    GenericClause::Contains(serde_json::json!({
      "io.nanocl.group": get_nanocl_group(state),
    })),
  );
  let old_secrets: Vec<SecretPartial> = cli_conf
    .client
    .list_secret(Some(&filter))
    .await?
    .iter()
    .map(|secret| secret.clone().into())
    .collect();
  let old_cargoes: Vec<CargoSpec> = cli_conf
    .client
    .list_cargo(Some(&GenericFilterNsp {
      filter: Some(filter.clone()),
      namespace: Some(
        state
          .data
          .namespace
          .clone()
          .unwrap_or_else(|| "global".to_owned()),
      ),
    }))
    .await?
    .iter()
    .map(|cargo| cargo_spec_from_revision(&cargo.spec))
    .collect();
  let old_vms: Vec<VmSpecPartial> = cli_conf
    .client
    .list_vm(Some(&GenericFilterNsp {
      filter: Some(filter.clone()),
      namespace: Some(
        state
          .data
          .namespace
          .clone()
          .unwrap_or_else(|| "global".to_owned()),
      ),
    }))
    .await?
    .iter()
    .map(|vm| vm.spec.clone().into())
    .collect();
  let old_resources: Vec<ResourcePartial> = cli_conf
    .client
    .list_resource(Some(&filter))
    .await?
    .iter()
    .map(|resource| resource.clone().into())
    .collect();
  let removed_secrets = state.data.secrets.as_ref().map(|secrets| {
    old_secrets
      .into_iter()
      .filter(|s| !secrets.iter().any(|ns| ns.name == s.name))
      .collect::<Vec<_>>()
  });
  let removed_cargoes = state.data.cargoes.as_ref().map(|cargoes| {
    old_cargoes
      .into_iter()
      .filter(|c| !cargoes.iter().any(|nc| nc.name == c.name))
      .collect::<Vec<_>>()
  });
  let removed_vms = state.data.virtual_machines.as_ref().map(|vms| {
    old_vms
      .into_iter()
      .filter(|v| !vms.iter().any(|nv| nv.name == v.name))
      .collect::<Vec<_>>()
  });
  let removed_resources = state.data.resources.as_ref().map(|resources| {
    old_resources
      .into_iter()
      .filter(|r| !resources.iter().any(|nr| nr.name == r.name))
      .collect::<Vec<_>>()
  });
  let old_state = StateRef {
    raw: "".to_owned(),
    format: state.format.clone(),
    data: Statefile {
      secrets: removed_secrets,
      cargoes: removed_cargoes,
      virtual_machines: removed_vms,
      resources: removed_resources,
      ..state.data.clone()
    },
    root: state.root.clone(),
    location: state.location.clone(),
  };
  state_remove(cli_conf, &old_state, json).await?;
  Ok(())
}

/// Function called when running `nanocl state apply`
pub(super) async fn exec_state_apply(
  cli_conf: &CliConfig,
  opts: &StateApplyOpts,
) -> IoResult<()> {
  let format = cli_conf.user_config.display_format.clone();
  let state_file = read_state_file(&opts.source, &format).await?;
  let args = parse_build_args(
    &state_file.data,
    ArgParseMode::Apply,
    &opts.args,
    opts.json,
  )?;
  let states =
    parse_state_file_recurr(cli_conf, &state_file, &args, false).await?;
  // Validate rendered schedules before submitting any jobs to the daemon.
  for state in &states {
    for job in state.data.jobs.iter().flatten() {
      job.validate_schedule().map_err(|err| {
        IoError::invalid_input(&format!("Job {} schedule", job.name), &err)
      })?;
    }
  }
  if !opts.skip_confirm {
    print_states(&states);
    utils::dialog::confirm("Are you sure to apply this state ?")
      .map_err(|err| err.map_err_context(|| "StateApply"))?;
  }
  for state in &states {
    if opts.remove_orphans {
      remove_orphans(cli_conf, state, opts.json).await?;
    }
    let output = opts.json.then(|| StateOutput {
      operation: "apply",
      statefile: Some(state.location.clone()),
    });
    let (progress, summary) = utils::progress::create_state_progress(
      state_item_count(state),
      "Applying",
      output.as_ref(),
    )?;
    let result =
      state_apply(cli_conf, opts, state, &progress, &summary, output.as_ref())
        .await;
    utils::progress::finish_state_progress(
      &summary,
      if result.is_ok() {
        "Applied"
      } else {
        "Apply failed"
      },
      result.is_err(),
      output.as_ref(),
    )?;
    result?;
  }
  if opts.follow {
    states
      .iter()
      .map(|state| async {
        state_logs(
          cli_conf,
          &StateLogsOpts {
            source: Some(state.root.to_string()),
            follow: true,
            ..Default::default()
          },
          state,
        )
        .await;
      })
      .collect::<FuturesUnordered<_>>()
      .collect::<Vec<_>>()
      .await;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use std::sync::{Arc, Mutex};

  use futures::{channel::oneshot, future::Either};
  use nanocld_client::{ConnectOpts, NANOCLD_DEFAULT_VERSION, NanocldClient};
  use ntex::{util::Bytes, web};

  use super::*;

  struct VmApplyRequests {
    paths: Vec<String>,
    conditions: Option<Value>,
    started: Option<oneshot::Sender<()>>,
    completion: Option<oneshot::Receiver<bool>>,
    reject_start: bool,
  }

  struct VmApplyFixture {
    client: NanocldClient,
    _server: web::test::TestServer,
    requests: Arc<Mutex<VmApplyRequests>>,
    started: oneshot::Receiver<()>,
    complete: oneshot::Sender<bool>,
  }

  fn vm_event(failed: bool, action: &str) -> Bytes {
    let event = serde_json::json!({
      "Key": "3e8f6eef-f03a-446d-9e7a-635728c3f565",
      "CreatedAt": "2026-01-01T00:00:00",
      "ExpiresAt": "2026-01-01T01:00:00",
      "ReportingNode": "test",
      "ReportingController": "nanocl.io/core",
      "Kind": if failed { "error" } else { "normal" },
      "Action": action,
      "Reason": "VM start",
      "Note": if failed { "VM start failed" } else { "VM started" },
      "Actor": {"Key": "global.guest", "Kind": "Vm"},
    });
    Bytes::from(format!("{event}\n"))
  }

  async fn watch_vm_start(
    req: web::HttpRequest,
    body: web::types::Json<Value>,
  ) -> web::HttpResponse {
    let requests = req.app_state::<Arc<Mutex<VmApplyRequests>>>().unwrap();
    let completion = {
      let mut requests = requests.lock().unwrap();
      requests.paths.push(req.path().to_owned());
      requests.conditions = Some(body.into_inner());
      requests.completion.take().unwrap()
    };
    let events = futures::stream::once(async {
      Ok::<_, std::io::Error>(vm_event(false, "starting"))
    })
    .chain(futures::stream::once(async move {
      let failed = completion.await.unwrap();
      Ok::<_, std::io::Error>(vm_event(failed, "start"))
    }));
    web::HttpResponse::Ok().streaming(Box::pin(events))
  }

  async fn start_matching_vm(req: web::HttpRequest) -> web::HttpResponse {
    let requests = req.app_state::<Arc<Mutex<VmApplyRequests>>>().unwrap();
    let mut requests = requests.lock().unwrap();
    requests.paths.push(req.path().to_owned());
    let _ = requests.started.take().unwrap().send(());
    if requests.reject_start {
      web::HttpResponse::Conflict()
        .json(&serde_json::json!({"msg": "VM start rejected"}))
    } else {
      web::HttpResponse::Accepted().into()
    }
  }

  fn vm_apply_fixture(reject_start: bool) -> VmApplyFixture {
    let (start, started) = oneshot::channel();
    let (complete, completion) = oneshot::channel();
    let requests = Arc::new(Mutex::new(VmApplyRequests {
      paths: Vec::new(),
      conditions: None,
      started: Some(start),
      completion: Some(completion),
      reject_start,
    }));
    let server_requests = requests.clone();
    let server = web::test::server(async move |_| {
      web::App::new()
        .with_config(
          web::WebAppConfig::new().set_state(server_requests.clone()),
        )
        .service(
          web::resource(format!("/v{NANOCLD_DEFAULT_VERSION}/events/watch"))
            .route(web::post().to(watch_vm_start)),
        )
        .service(
          web::resource(format!(
            "/v{NANOCLD_DEFAULT_VERSION}/processes/vm/global.guest/start"
          ))
          .route(web::post().to(start_matching_vm)),
        )
    });
    let client = NanocldClient::connect_to(&ConnectOpts {
      url: format!("http://localhost:{}", server.addr().port()),
      ..Default::default()
    })
    .unwrap();
    VmApplyFixture {
      client,
      _server: server,
      requests,
      started,
      complete,
    }
  }

  async fn stopped_vm_apply(failed: bool) -> IoResult<&'static str> {
    let fixture = vm_apply_fixture(false);
    let pg = ProgressBar::hidden();
    let summary = ProgressBar::hidden();
    let apply = apply_matching_vm(
      &fixture.client,
      "global.guest",
      &ObjPsStatusKind::Stop,
      &pg,
      &summary,
      None,
    );
    futures::pin_mut!(apply);
    match futures::future::select(apply.as_mut(), fixture.started).await {
      Either::Left((result, _)) => {
        panic!("apply finished before requesting start: {result:?}");
      }
      Either::Right((started, _)) => started.unwrap(),
    }
    // The accepted HTTP request alone cannot produce a successful apply.
    assert!(futures::poll!(apply.as_mut()).is_pending());
    fixture.complete.send(failed).unwrap();
    let result = apply.await;
    let requests = fixture.requests.lock().unwrap();
    assert_eq!(
      requests.paths,
      [
        format!("/v{NANOCLD_DEFAULT_VERSION}/events/watch"),
        format!("/v{NANOCLD_DEFAULT_VERSION}/processes/vm/global.guest/start"),
      ]
    );
    assert_eq!(
      requests.conditions,
      Some(serde_json::json!([{
        "ActorKey": "global.guest",
        "ActorKind": "Vm",
        "Kind": ["normal", "error"],
        "Action": ["start"],
      }]))
    );
    result
  }

  #[ntex::test]
  async fn vm_state_apply_matching_running_is_unchanged() {
    let fixture = vm_apply_fixture(false);
    let result = apply_matching_vm(
      &fixture.client,
      "global.guest",
      &ObjPsStatusKind::Start,
      &ProgressBar::hidden(),
      &ProgressBar::hidden(),
      None,
    )
    .await
    .unwrap();
    assert_eq!(result, "Unchanged");
    assert!(fixture.requests.lock().unwrap().paths.is_empty());
  }

  #[ntex::test]
  async fn vm_state_apply_matching_stopped_waits_for_start() {
    assert_eq!(stopped_vm_apply(false).await.unwrap(), "Running");
  }

  #[ntex::test]
  async fn vm_state_apply_matching_stopped_propagates_start_failure() {
    let error = stopped_vm_apply(true).await.unwrap_err();
    assert!(error.to_string().contains("VM start failed"));
  }

  #[ntex::test]
  async fn vm_state_apply_matching_stopped_propagates_start_rejection() {
    let fixture = vm_apply_fixture(true);
    let result = apply_matching_vm(
      &fixture.client,
      "global.guest",
      &ObjPsStatusKind::Stop,
      &ProgressBar::hidden(),
      &ProgressBar::hidden(),
      None,
    )
    .await;
    // Close the fixture's event stream after the rejected request.
    fixture.complete.send(false).unwrap();
    assert!(
      result
        .unwrap_err()
        .to_string()
        .contains("VM start rejected")
    );
    assert_eq!(fixture.requests.lock().unwrap().paths.len(), 2);
  }

  #[test]
  fn vm_state_apply_preserves_generated_identity_and_omitted_options() {
    let current = VmSpecPartial {
      name: "guest".to_owned(),
      image: "/vm.qcow2".to_owned(),
      mac_address: Some("02:ab:cd:ef:00:01".to_owned()),
      cloud_init: Some(false),
      cloud_init_user_data: Some(
        serde_json::from_value(serde_json::json!({"packages": ["curl"]}))
          .unwrap(),
      ),
      console: Some(true),
      ..Default::default()
    };
    let mut desired = VmSpecPartial {
      name: current.name.clone(),
      image: current.image.clone(),
      ..Default::default()
    };
    assert_eq!(vm_spec_for_comparison(&desired, &current).unwrap(), current);
    let payload =
      serde_json::to_value(VmSpecUpdate::from(desired.clone())).unwrap();
    for field in ["MacAddress", "CloudInit", "CloudInitUserData", "Console"] {
      assert!(payload.get(field).is_none(), "{field} must remain omitted");
    }
    desired.console = Some(false);
    desired.cloud_init_user_data = Some(String::new().into());
    let comparison = vm_spec_for_comparison(&desired, &current).unwrap();
    assert_eq!(comparison.console, Some(false));
    assert_eq!(comparison.cloud_init_user_data, Some(String::new().into()));
    assert_eq!(desired.console, Some(false));
    assert_eq!(desired.cloud_init_user_data, Some(String::new().into()));
    desired.mac_address = Some("02:00:00:00:00:02".to_owned());
    assert!(vm_spec_for_comparison(&desired, &current).is_err());
    let mut invalid_current = current.clone();
    invalid_current.mac_address = None;
    assert!(vm_spec_for_comparison(&desired, &invalid_current).is_err());
    desired.mac_address = None;
    assert!(vm_spec_for_comparison(&desired, &invalid_current).is_err());
  }

  #[test]
  fn vm_state_apply_explicit_null_clears_user_data_without_mutating_payload() {
    let current = VmSpecPartial {
      name: "guest".into(),
      image: "/vm.qcow2".into(),
      mac_address: Some("02:ab:cd:ef:00:01".into()),
      cloud_init_user_data: Some("#cloud-config\n".to_owned().into()),
      ..Default::default()
    };
    let desired: VmSpecPartial = serde_json::from_value(serde_json::json!({
      "Name": "guest", "Image": "/vm.qcow2", "CloudInitUserData": null,
    }))
    .unwrap();
    let comparison = vm_spec_for_comparison(&desired, &current).unwrap();
    assert!(comparison.cloud_init_user_data.is_none());
    assert_ne!(comparison, current);
    let payload = serde_json::to_value(VmSpecUpdate::from(desired)).unwrap();
    assert_eq!(payload.get("CloudInitUserData"), Some(&Value::Null));
  }
}
