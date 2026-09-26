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
                if (cmp != vm) || opts.reload {
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
