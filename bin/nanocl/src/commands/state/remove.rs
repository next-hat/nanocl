use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocld_client::stubs::statefile::Statefile;

use crate::{
  commands::GenericCommandRm,
  config::CliConfig,
  models::{
    CargoArg, GenericDefaultOpts, GenericRemoveForceOpts, GenericRemoveOpts,
    JobArg, ResourceArg, SecretArg, StateOutput, StateRef, StateRemoveOpts,
    VmArg,
  },
  utils,
};

use super::{
  ArgParseMode, parse_build_args, parse_state_file_recurr, print_states,
  read_state_file, state_item_count,
};

pub(super) async fn state_remove(
  cli_conf: &CliConfig,
  state_file: &StateRef<Statefile>,
  json: bool,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let output = json.then(|| StateOutput {
    operation: "remove",
    statefile: Some(state_file.location.clone()),
  });
  let (progress, summary) = utils::progress::create_state_progress(
    state_item_count(state_file),
    "Removing",
    output.as_ref(),
  )?;
  let mut failures = 0;
  let result = async {
    let namespace = match &state_file.data.namespace {
      None => "global",
      Some(namespace) => namespace,
    };
    let mut gen_rm_opts = GenericRemoveOpts::<GenericDefaultOpts> {
      keys: Vec::default(),
      skip_confirm: true,
      others: GenericDefaultOpts,
    };
    if let Some(jobs) = &state_file.data.jobs {
      gen_rm_opts.keys = jobs.iter().map(|job| job.name.clone()).collect();
      match JobArg::exec_rm_with_progress(
        client,
        &gen_rm_opts,
        Some((&progress, &summary)),
        output.as_ref(),
      )
      .await
      {
        Ok(count) => failures += count,
        Err(err) => {
          if json {
            return Err(err);
          }
          failures += 1;
          progress.suspend(|| eprintln!("Error while removing jobs {err}"));
        }
      }
    }
    if let Some(cargoes) = &state_file.data.cargoes {
      let opts = GenericRemoveOpts::<GenericRemoveForceOpts> {
        keys: cargoes
          .iter()
          .map(|cargo| utils::process::resource_key(&cargo.name, namespace))
          .collect::<IoResult<Vec<_>>>()?,
        skip_confirm: true,
        others: GenericRemoveForceOpts { force: true },
      };
      match CargoArg::exec_rm_with_progress(
        client,
        &opts,
        Some((&progress, &summary)),
        output.as_ref(),
      )
      .await
      {
        Ok(count) => failures += count,
        Err(err) => {
          if json {
            return Err(err);
          }
          failures += 1;
          progress.suspend(|| eprintln!("Error while removing cargoes {err}"));
        }
      }
    }
    if let Some(vms) = &state_file.data.virtual_machines {
      gen_rm_opts.keys = vms
        .iter()
        .map(|vm| utils::process::resource_key(&vm.name, namespace))
        .collect::<IoResult<Vec<_>>>()?;
      match VmArg::exec_rm_with_progress(
        client,
        &gen_rm_opts,
        Some((&progress, &summary)),
        output.as_ref(),
      )
      .await
      {
        Ok(count) => failures += count,
        Err(err) => {
          if json {
            return Err(err);
          }
          failures += 1;
          progress.suspend(|| eprintln!("Error while removing vms {err}"));
        }
      }
    }
    if let Some(resources) = &state_file.data.resources {
      gen_rm_opts.keys = resources
        .iter()
        .map(|resource| resource.name.clone())
        .collect();
      match ResourceArg::exec_rm_with_progress(
        client,
        &gen_rm_opts,
        Some((&progress, &summary)),
        output.as_ref(),
      )
      .await
      {
        Ok(count) => failures += count,
        Err(err) => {
          if json {
            return Err(err);
          }
          failures += 1;
          progress
            .suspend(|| eprintln!("Error while removing resources {err}"));
        }
      }
    }
    if let Some(secrets) = &state_file.data.secrets {
      gen_rm_opts.keys =
        secrets.iter().map(|secret| secret.name.clone()).collect();
      match SecretArg::exec_rm_with_progress(
        client,
        &gen_rm_opts,
        Some((&progress, &summary)),
        output.as_ref(),
      )
      .await
      {
        Ok(count) => failures += count,
        Err(err) => {
          if json {
            return Err(err);
          }
          failures += 1;
          progress.suspend(|| eprintln!("Error while removing secrets {err}"));
        }
      }
    }
    Ok::<_, IoError>(())
  }
  .await;
  let failed = failures > 0 || result.is_err();
  utils::progress::finish_state_progress(
    &summary,
    if failed { "Remove failed" } else { "Removed" },
    failed,
    output.as_ref(),
  )?;
  result?;
  if json && failures > 0 {
    return Err(IoError::other(
      "StateRemove",
      &format!("{failures} item(s) could not be removed"),
    ));
  }
  Ok(())
}

/// Function called when running `nanocl state rm`
pub(super) async fn exec_state_remove(
  cli_conf: &CliConfig,
  opts: &StateRemoveOpts,
) -> IoResult<()> {
  let format = cli_conf.user_config.display_format.clone();
  let state_file = read_state_file(&opts.source, &format).await?;
  let args = parse_build_args(
    &state_file.data,
    ArgParseMode::Remove,
    &opts.args,
    opts.json,
  )?;
  let state_files =
    parse_state_file_recurr(cli_conf, &state_file, &args, false).await?;
  if !opts.skip_confirm {
    print_states(&state_files);
    utils::dialog::confirm("Are you sure to remove this state ?")
      .map_err(|err| err.map_err_context(|| "Delete resource"))?;
  }
  for state in &state_files {
    state_remove(cli_conf, state, opts.json).await?;
  }
  Ok(())
}
