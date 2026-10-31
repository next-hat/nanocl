use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocld_client::stubs::statefile::Statefile;

use crate::{
  commands::GenericCommandStart,
  config::CliConfig,
  models::{
    CargoArg, GenericStartOpts, JobArg, StateOutput, StateRef, StateStartOpts,
    VmArg,
  },
  utils,
};

use super::{
  ArgParseMode, parse_build_args, parse_state_file_recurr, print_states,
  read_state_file,
};

fn start_targets(
  state: &StateRef<Statefile>,
) -> IoResult<Vec<(&'static str, GenericStartOpts)>> {
  let namespace = state.data.namespace.as_deref().unwrap_or("global");
  Ok(vec![
    (
      "jobs",
      GenericStartOpts {
        keys: state
          .data
          .jobs
          .iter()
          .flatten()
          .map(|job| job.name.clone())
          .collect(),
      },
    ),
    (
      "cargoes",
      GenericStartOpts {
        keys: state
          .data
          .cargoes
          .iter()
          .flatten()
          .map(|cargo| utils::process::resource_key(&cargo.name, namespace))
          .collect::<IoResult<Vec<_>>>()?,
      },
    ),
    (
      "vms",
      GenericStartOpts {
        keys: state
          .data
          .virtual_machines
          .iter()
          .flatten()
          .map(|vm| utils::process::resource_key(&vm.name, namespace))
          .collect::<IoResult<Vec<_>>>()?,
      },
    ),
  ])
}

async fn state_start(
  cli_conf: &CliConfig,
  state: &StateRef<Statefile>,
  json: bool,
) -> IoResult<()> {
  let targets = start_targets(state)?;
  let output = json.then(|| StateOutput {
    operation: "start",
    statefile: Some(state.location.clone()),
  });
  let total = targets.iter().map(|(_, opts)| opts.keys.len() as u64).sum();
  let (progress, summary) =
    utils::progress::create_state_progress(total, "Starting", output.as_ref())?;
  let mut failures = 0;
  let result = async {
    for (kind, opts) in &targets {
      let bars = Some((&progress, &summary));
      let result = match *kind {
        "jobs" => {
          JobArg::exec_start_with_progress(
            &cli_conf.client,
            opts,
            bars,
            output.as_ref(),
          )
          .await
        }
        "cargoes" => {
          CargoArg::exec_start_with_progress(
            &cli_conf.client,
            opts,
            bars,
            output.as_ref(),
          )
          .await
        }
        "vms" => {
          VmArg::exec_start_with_progress(
            &cli_conf.client,
            opts,
            bars,
            output.as_ref(),
          )
          .await
        }
        _ => unreachable!(),
      };
      match result {
        Ok(count) => failures += count,
        Err(err) => {
          if json {
            return Err(err);
          }
          failures += 1;
          progress.suspend(|| eprintln!("Error while starting {kind} {err}"));
        }
      }
    }
    Ok::<_, IoError>(())
  }
  .await;
  let failed = failures > 0 || result.is_err();
  utils::progress::finish_state_progress(
    &summary,
    if failed { "Start failed" } else { "Started" },
    failed,
    output.as_ref(),
  )?;
  result?;
  if failures > 0 {
    return Err(IoError::other(
      "StateStart",
      &format!("{failures} item(s) could not be started"),
    ));
  }
  Ok(())
}

/// Function called when running `nanocl state start`.
pub(super) async fn exec_state_start(
  cli_conf: &CliConfig,
  opts: &StateStartOpts,
) -> IoResult<()> {
  let state =
    read_state_file(&opts.source, &cli_conf.user_config.display_format).await?;
  let args =
    parse_build_args(&state.data, ArgParseMode::Start, &opts.args, opts.json)?;
  let states = parse_state_file_recurr(cli_conf, &state, &args, true).await?;
  if !opts.skip_confirm {
    print_states(&states);
    utils::dialog::confirm("Are you sure to start this state ?")
      .map_err(|err| err.map_err_context(|| "Start state"))?;
  }
  for state in &states {
    state_start(cli_conf, state, opts.json).await?;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn state(namespace: Option<&str>) -> StateRef<Statefile> {
    StateRef {
      raw: String::new(),
      format: Default::default(),
      data: serde_json::from_value(serde_json::json!({
        "ApiVersion": "v0.19",
        "Namespace": namespace,
        "Cargoes": [{"Name": "app", "Containers": []}],
        "VirtualMachines": [{"Name": "app", "Image": "/tmp/vm.img"}],
        "Jobs": [{"Name": "migrate", "Containers": []}],
        "Resources": [{"Name": "route", "Kind": "ProxyRule", "Data": {}}],
        "Secrets": [{"Name": "credentials", "Kind": "Env", "Data": {}}],
      }))
      .unwrap(),
      root: Default::default(),
      location: "Statefile.yml".into(),
    }
  }

  #[test]
  fn start_targets_include_only_workloads_with_namespaced_keys() {
    for (namespace, expected) in
      [(None, "global"), (Some("production"), "production")]
    {
      let targets = start_targets(&state(namespace)).unwrap();
      let keys = targets
        .into_iter()
        .map(|(kind, opts)| (kind, opts.keys))
        .collect::<Vec<_>>();
      assert_eq!(
        keys,
        vec![
          ("jobs", vec!["migrate".into()]),
          ("cargoes", vec![format!("{expected}.app")]),
          ("vms", vec![format!("{expected}.app")]),
        ]
      );
    }
    let mut empty = state(None);
    empty.data.jobs = None;
    empty.data.cargoes = None;
    empty.data.virtual_machines = None;
    assert!(
      start_targets(&empty)
        .unwrap()
        .iter()
        .all(|(_, opts)| opts.keys.is_empty())
    );
  }

  #[test]
  fn start_targets_reject_invalid_resource_names() {
    let mut invalid = state(None);
    invalid.data.cargoes.as_mut().unwrap()[0].name = "invalid.name".into();
    assert!(start_targets(&invalid).is_err());
    let mut invalid = state(None);
    invalid.data.virtual_machines.as_mut().unwrap()[0].name =
      "invalid.name".into();
    assert!(start_targets(&invalid).is_err());
  }
}
