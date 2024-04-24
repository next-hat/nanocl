use std::{
  fs,
  env::{consts, vars_os},
  path::{Path, PathBuf},
  collections::HashMap,
};

use ntex::rt;
use serde_json::{Map, Value};
use clap::{Arg, Command, ArgAction};
use futures::{
  join, StreamExt,
  stream::{FuturesOrdered, FuturesUnordered},
};
use async_recursion::async_recursion;

use nanocl_error::io::{IoError, FromIo, IoResult};

use nanocld_client::{
  ConnectOpts,
  stubs::{
    statefile::SubState,
    process::Process,
    system::{EventActorKind, EventCondition, EventKind, ObjPsStatusKind},
  },
};

use nanocld_client::{
  NanocldClient,
  stubs::{
    job::JobPartial,
    statefile::Statefile,
    process::ProcessLogQuery,
    cargo_spec::CargoSpecPartial,
    vm_spec::{VmSpecPartial, VmSpecUpdate},
    resource::{ResourcePartial, ResourceUpdate},
    secret::{SecretUpdate, SecretPartial},
    cargo::CargoDeleteQuery,
    system::NativeEventAction,
  },
};

use crate::{
  utils,
  config::CliConfig,
  models::{
    Context, DisplayFormat, StateApplyOpts, StateArg, StateCommand,
    StateLogsOpts, StateRef, StateRemoveOpts, StateRoot,
  },
};

/// Get Statefile from url and return a StateRef with the raw data and the format
async fn get_from_url(
  url: &str,
  format: &DisplayFormat,
) -> IoResult<StateRef<Statefile>> {
  let (url, data) = utils::state::download_statefile(url).await?;
  let ext = utils::state::get_format(format, url.clone());
  let mut root = url.split('/').map(str::to_string).collect::<Vec<String>>();
  root.pop();
  root.push("".to_owned());
  let root = root.join("/");
  let state_ref =
    utils::state::get_state_ref(&ext, &data, StateRoot::Url(root))?;
  Ok(state_ref)
}

/// Read Statefile from file and return a StateRef with the raw data and the format
fn read_from_file<T>(
  path: &PathBuf,
  format: &DisplayFormat,
) -> IoResult<StateRef<T>>
where
  T: serde::Serialize + serde::de::DeserializeOwned,
{
  let data = fs::read_to_string(path)?;
  let mut include_path = path.clone();
  include_path.pop();
  let ext = utils::state::get_format(format, path);
  let state_ref = utils::state::get_state_ref::<T>(
    &ext,
    &data,
    StateRoot::File(include_path),
  )?;
  Ok(state_ref)
}

async fn wait_job_instance_and_log(
  client: &NanocldClient,
  instance: &Process,
  query: &ProcessLogQuery,
) {
  let Ok(mut stream) = client.watch_events(None).await else {
    return;
  };
  while let Some(event) = stream.next().await {
    let Ok(event) = event else {
      continue;
    };
    if event.action != NativeEventAction::Start.to_string() {
      continue;
    };
    let Some(actor) = event.actor else {
      continue;
    };
    let Some(key) = actor.key else {
      continue;
    };
    if key != instance.name {
      continue;
    }
    match client.logs_process(&key, Some(query)).await {
      Err(err) => {
        eprintln!("Cannot get job instance {key} logs: {err}");
      }
      Ok(stream) => {
        if let Err(err) = utils::print::logs_process_stream(stream).await {
          eprintln!("{err}");
        }
      }
    }
    break;
  }
}

/// Logs existing jobs in the Statefile
async fn log_jobs(
  client: &NanocldClient,
  jobs: Vec<JobPartial>,
  query: &ProcessLogQuery,
) {
  jobs
    .iter()
    .map(|job| async move {
      let job = match client.inspect_job(&job.name).await {
        Ok(job) => job,
        Err(err) => {
          eprintln!("Unable to inspect job {}: {err}", job.name);
          return;
        }
      };
      job
        .instances
        .iter()
        .map(|instance| async move {
          let started_at =
            instance.clone().data.state.unwrap_or_default().started_at;
          match started_at {
            None => {
              wait_job_instance_and_log(client, instance, query).await;
            }
            Some(started_at) => {
              if started_at == "0001-01-01T00:00:00Z" {
                wait_job_instance_and_log(client, instance, query).await;
                return;
              }
              let stream = client
                .logs_process(&instance.name, Some(query))
                .await
                .unwrap();
              if let Err(err) = utils::print::logs_process_stream(stream).await
              {
                eprintln!("{err}");
              }
            }
          }
        })
        .collect::<FuturesUnordered<_>>()
        .collect::<Vec<_>>()
        .await;
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// Attach to a list of cargoes and print their logs
pub async fn log_cargoes(
  client: &NanocldClient,
  cargoes: Vec<CargoSpecPartial>,
  query: &ProcessLogQuery,
) {
  cargoes
    .into_iter()
    .map(|cargo| async move {
      match client
        .logs_processes("cargo", &cargo.name, Some(query))
        .await
      {
        Err(err) => {
          eprintln!("Cannot attach to cargo {}: {err}", &cargo.name);
        }
        Ok(stream) => {
          if let Err(err) = utils::print::logs_process_stream(stream).await {
            eprintln!("{err}");
          }
        }
      }
    })
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
}

/// Hook cargoes binds to replace relative path with absolute path
fn hook_cargoes(
  cargoes: Vec<CargoSpecPartial>,
) -> IoResult<Vec<CargoSpecPartial>> {
  let mut new_cargoes = Vec::new();
  for cargo in cargoes {
    let new_cargo = utils::docker::hook_binds(&cargo)?;
    new_cargoes.push(new_cargo);
  }
  Ok(new_cargoes)
}

/// Generate a nanocl daemon client based on the api version specified in the Statefile
fn gen_client(
  cli_conf: &CliConfig,
  state_ref: &StateRef<Statefile>,
) -> IoResult<NanocldClient> {
  let client = match &state_ref.data.api_version {
    api_version if state_ref.data.api_version.starts_with("http") => {
      let mut paths = api_version
        .split('/')
        .map(|e| e.to_owned())
        .collect::<Vec<String>>();
      // extract and remove last item of paths
      let path_ptr = paths.clone();
      let version = path_ptr
        .last()
        .ok_or(IoError::not_found("Version", "is not specified"))?;
      paths.remove(paths.len() - 1);
      let url = paths.join("/");
      NanocldClient::connect_to(&ConnectOpts {
        url,
        version: Some(version.into()),
        ..Default::default()
      })
    }
    api_version if state_ref.data.api_version.starts_with('v') => {
      NanocldClient::connect_to(&ConnectOpts {
        url: cli_conf.host.clone(),
        ssl: cli_conf
          .context
          .endpoints
          .get("Nanocl")
          .expect("Nanocl endpoint is not defined")
          .ssl
          .clone(),
        version: Some(api_version.clone()),
      })
    }
    _ => {
      let mut paths = state_ref
        .data
        .api_version
        .split('/')
        .map(|e| e.to_owned())
        .collect::<Vec<String>>();
      // extract and remove last item of paths
      let path_ptr = paths.clone();
      let version = path_ptr
        .last()
        .ok_or(IoError::not_found("Version", "is not specified"))?;
      paths.remove(paths.len() - 1);
      let url = paths.join("/");
      let url = format!("https://{url}");
      NanocldClient::connect_to(&ConnectOpts {
        url,
        version: Some(version.into()),
        ..Default::default()
      })
    }
  };
  Ok(client)
}

/// Parse `Args` from a Statefile and ask the user to input their values
fn parse_build_args(
  state_file: &Statefile,
  args: &[String],
) -> IoResult<serde_json::Value> {
  let mut cmd = Command::new("nanocl state args")
    .about("Validate state args")
    .bin_name("nanocl state args --");
  // Add string nanocl state args as first element of args
  let mut args = args.to_owned();
  args.insert(0, "nanocl state apply --".into());
  for build_arg in state_file.args.clone().unwrap_or_default() {
    let name = build_arg.name.to_owned();
    let arg: &'static str = Box::leak(name.into_boxed_str());
    let mut cmd_arg = Arg::new(arg).long(arg);
    match build_arg.default {
      Some(default) => {
        if build_arg.kind != "Boolean" {
          let default_value: &'static str = Box::leak(default.into_boxed_str());
          cmd_arg = cmd_arg.default_value(default_value);
        }
      }
      None => {
        if build_arg.kind == "Boolean" {
          cmd_arg = cmd_arg.action(ArgAction::SetTrue);
        } else {
          cmd_arg = cmd_arg.required(true);
        }
      }
    }
    cmd = cmd.arg(cmd_arg);
  }
  let matches = cmd.get_matches_from(args);
  let mut args = Map::new();
  for build_arg in state_file.args.clone().unwrap_or_default() {
    let name = build_arg.name.to_owned();
    let arg: &'static str = Box::leak(name.to_owned().into_boxed_str());
    match build_arg.kind.as_str() {
      "String" => {
        let value =
          matches.get_one::<String>(arg).ok_or(IoError::invalid_data(
            "BuildArg".into(),
            format!("argument {arg} is missing"),
          ))?;
        args.insert(name, Value::String(value.to_owned()));
      }
      "Boolean" => {
        let value = matches.get_flag(&name);
        args.insert(name, Value::Bool(value));
      }
      "Number" => {
        let value =
          matches.get_one::<String>(arg).ok_or(IoError::invalid_data(
            "BuildArg".into(),
            format!("argument {arg} is missing"),
          ))?;
        let value = value.parse::<usize>().map_err(|err| {
          IoError::invalid_data(
            "BuildArg".into(),
            format!("argument {arg} is not a number: {err}"),
          )
        })?;
        args.insert(name, Value::Number(value.into()));
      }
      _ => {
        return Err(IoError::invalid_data(
          "Statefile".into(),
          format!("unknown type {}", build_arg.kind),
        ))
      }
    }
  }
  let args = Value::Object(args);
  Ok(args)
}

/// Inject `Args` to the namespace value
fn inject_namespace(
  namespace: &str,
  args: &serde_json::Value,
) -> IoResult<String> {
  let object = liquid::object!({
    "Args": args,
  });
  let str = utils::state::compile(namespace, &object, StateRoot::None)?;
  Ok(str)
}

/// Inject `Args`, `Envs`, `Config`, `HostGateway` and `Namespaces` to the Statefile
async fn inject_data(
  state_ref: &StateRef<Statefile>,
  args: &serde_json::Value,
  context: &Context,
  client: &NanocldClient,
) -> IoResult<StateRef<Statefile>> {
  let mut envs = HashMap::new();
  for (key, value) in vars_os() {
    let key = key.to_string_lossy().to_string();
    let value = value.to_string_lossy().to_string();
    envs.insert(key, value);
  }
  let info = client.info().await?;
  let namespaces = client.list_namespace().await?.into_iter().fold(
    HashMap::new(),
    |mut acc, elem| {
      acc.insert(elem.name.clone(), elem);
      acc
    },
  );
  let data = liquid::object!({
    "Args": args,
    "Envs": envs,
    "Context": context,
    "Os": consts::OS,
    "OsFamily": consts::FAMILY,
    "Config": info.config,
    "HostGateway": info.host_gateway,
    "Namespaces": namespaces,
    "StateRoot": state_ref.root.to_string(),
  });
  let raw =
    utils::state::compile(&state_ref.raw, &data, state_ref.root.clone())?;
  let state_file =
    utils::state::serialize_ext::<Statefile>(&state_ref.format, &raw)?;
  Ok(StateRef {
    raw,
    format: state_ref.format.clone(),
    data: state_file,
    root: state_ref.root.clone(),
  })
}

/// Parse a Statefile from a path or url and return a StateRef with the raw data and the format
async fn read_state_file(
  path: &Option<String>,
  format: &DisplayFormat,
) -> IoResult<StateRef<Statefile>> {
  if let Some(path) = path {
    if let Ok(path) = Path::new(&path)
      .canonicalize()
      .map_err(|err| err.map_err_context(|| format!("Statefile {path}")))
    {
      return read_from_file(&path, format);
    }
    return get_from_url(path, format).await;
  }
  if let Ok(path) = Path::new("Statefile.yaml").canonicalize() {
    return read_from_file(&path, format);
  }
  if let Ok(path) = Path::new("Statefile").canonicalize() {
    return read_from_file(&path, format);
  }
  let path = Path::new("Statefile.yml")
    .canonicalize()
    .map_err(|err| err.map_err_context(|| "Statefile Statefile.yml"))?;
  read_from_file(&path, format)
}

async fn render_template(
  state_ref: &StateRef<Statefile>,
  args: &serde_json::Value,
  client: &NanocldClient,
  cli_conf: &CliConfig,
) -> IoResult<StateRef<Statefile>> {
  let mut namespace = match &state_ref.data.namespace {
    Some(namespace) => namespace.clone(),
    None => "global".to_owned(),
  };
  namespace = inject_namespace(&namespace, args)?;
  if client.inspect_namespace(&namespace).await.is_err() {
    client.create_namespace(&namespace).await?;
  }
  let mut state_ref =
    inject_data(state_ref, args, &cli_conf.context, client).await?;
  state_ref.data.namespace = Some(namespace);
  if let Some(cargoes) = state_ref.data.cargoes {
    let hooked_cargoes = hook_cargoes(cargoes)?;
    state_ref.data.cargoes = Some(hooked_cargoes);
  }
  Ok(state_ref)
}

async fn wait_process_object(
  key: &str,
  kind: EventActorKind,
  action: Vec<NativeEventAction>,
  client: &NanocldClient,
) -> IoResult<rt::JoinHandle<IoResult<()>>> {
  let mut stream = client
    .watch_events(Some(vec![EventCondition {
      actor_key: Some(key.to_owned()),
      actor_kind: Some(kind.clone()),
      kind: vec![EventKind::Normal, EventKind::Error],
      action,
      ..Default::default()
    }]))
    .await?;
  let fut = rt::spawn(async move {
    while let Some(event) = stream.next().await {
      let event = event?;
      if event.kind == EventKind::Error {
        return Err(IoError::interrupted(
          "Error",
          &event.note.unwrap_or_default(),
        ));
      }
    }
    Ok::<_, IoError>(())
  });
  Ok(fut)
}

#[async_recursion(?Send)]
async fn parse_state_file_recurr(
  cli_conf: &CliConfig,
  location: &Option<String>,
  cli_args: &[String],
) -> IoResult<Vec<StateRef<Statefile>>> {
  let format = cli_conf.user_config.display_format.clone();
  let state_ref = read_state_file(location, &format).await?;
  let client = gen_client(cli_conf, &state_ref)?;
  let args = parse_build_args(&state_ref.data, cli_args)?;
  let state_file =
    render_template(&state_ref, &args, &client, cli_conf).await?;
  let sub_states = state_file.data.sub_states.clone().unwrap_or_default();
  let parsed_sub_states = sub_states
    .iter()
    .map(|sub_state| async move {
      let current = PathBuf::from(location.clone().unwrap_or_default())
        .canonicalize()
        .unwrap();
      let parent = current.parent().unwrap();
      let path = match sub_state {
        SubState::Path(path) => path,
        SubState::Definition(sub_state) => &sub_state.path,
      };
      let full_path = parent.join(path).to_str().unwrap().to_owned();
      parse_state_file_recurr(cli_conf, &Some(full_path), cli_args).await
    })
    .collect::<FuturesOrdered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
  let mut states = vec![state_file.clone()];
  states.append(&mut parsed_sub_states.into_iter().flatten().collect());
  states.reverse();
  Ok(states)
}

async fn state_apply(
  cli_conf: &CliConfig,
  opts: &StateApplyOpts,
  state_file: &StateRef<Statefile>,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let namespace = state_file.data.namespace.clone().unwrap_or("global".into());
  let pg_style = utils::progress::create_spinner_style("green");
  if let Some(secrets) = &state_file.data.secrets {
    for secret in secrets {
      let token = format!("secret/{}", secret.name);
      let pg = utils::progress::create_progress(&token, &pg_style);
      match client.inspect_secret(&secret.name).await {
        Err(_) => {
          client.create_secret(secret).await?;
        }
        Ok(inspect) => {
          let cmp: SecretPartial = inspect.into();
          if cmp != *secret {
            let update: SecretUpdate = secret.clone().into();
            client.patch_secret(&secret.name, &update).await?;
          }
        }
      }
      pg.finish();
    }
  }
  if let Some(jobs) = &state_file.data.jobs {
    for job in jobs {
      let token = format!("job/{}", job.name);
      let pg = utils::progress::create_progress(&token, &pg_style);
      if client.inspect_job(&job.name).await.is_ok() {
        let waiter = wait_process_object(
          &job.name,
          EventActorKind::Job,
          vec![NativeEventAction::Destroy],
          client,
        )
        .await?;
        client.delete_job(&job.name).await?;
        waiter.await??;
      }
      client.create_job(job).await?;
      let waiter = wait_process_object(
        &job.name,
        EventActorKind::Job,
        vec![NativeEventAction::Start],
        client,
      )
      .await?;
      client.start_process("job", &job.name, None).await?;
      waiter.await??;
      pg.finish();
    }
  }
  if let Some(cargoes) = &state_file.data.cargoes {
    for cargo in cargoes {
      let token = format!("cargo/{}", cargo.name);
      let pg = utils::progress::create_progress(&token, &pg_style);
      match client.inspect_cargo(&cargo.name, Some(&namespace)).await {
        Err(_) => {
          client.create_cargo(cargo, Some(&namespace)).await?;
        }
        Ok(inspect) => {
          if inspect.status.actual == ObjPsStatusKind::Start && !opts.reload {
            pg.finish();
            continue;
          }
          let cmp: CargoSpecPartial = inspect.spec.into();
          if cmp != *cargo || opts.reload {
            client
              .put_cargo(&cargo.name, cargo, Some(&namespace))
              .await?;
          }
        }
      }
      let waiter = wait_process_object(
        &format!("{}.{namespace}", cargo.name),
        EventActorKind::Cargo,
        vec![NativeEventAction::Start],
        client,
      )
      .await?;
      client
        .start_process("cargo", &cargo.name, Some(&namespace))
        .await?;
      waiter.await??;
      pg.finish();
    }
  }
  if let Some(vms) = &state_file.data.virtual_machines {
    for vm in vms {
      let token = format!("vm/{}", vm.name);
      let pg = utils::progress::create_progress(&token, &pg_style);
      match client.inspect_vm(&vm.name, Some(&namespace)).await {
        Err(_) => {
          client.create_vm(vm, Some(&namespace)).await?;
        }
        Ok(inspect) => {
          if inspect.status.actual == ObjPsStatusKind::Start && !opts.reload {
            pg.finish();
            continue;
          }
          let cmp: VmSpecPartial = inspect.spec.into();
          if cmp != *vm {
            let update: VmSpecUpdate = vm.clone().into();
            client.patch_vm(&vm.name, &update, Some(&namespace)).await?;
          }
        }
      }
      let waiter = wait_process_object(
        &format!("{}.{namespace}", vm.name),
        EventActorKind::Vm,
        vec![NativeEventAction::Start],
        client,
      )
      .await?;
      client
        .start_process("vm", &vm.name, Some(&namespace))
        .await?;
      waiter.await??;
      pg.finish();
    }
  }
  if let Some(resources) = &state_file.data.resources {
    for resource in resources {
      let token = format!("resource/{}", resource.name);
      let pg = utils::progress::create_progress(&token, &pg_style);
      match client.inspect_resource(&resource.name).await {
        Err(_) => {
          client.create_resource(resource).await?;
        }
        Ok(inspect) => {
          let cmp: ResourcePartial = inspect.into();
          if cmp != *resource {
            let update: ResourceUpdate = resource.clone().into();
            client.put_resource(&resource.name, &update).await?;
          }
        }
      }
      pg.finish();
    }
  }
  Ok(())
}

fn print_states(states: &[StateRef<Statefile>]) {
  let raw = states.iter().fold(String::new(), |init, state| {
    format!("{init}{}\n", state.raw)
  });
  println!("{raw}");
}

/// Function called when running `nanocl state apply`
async fn exec_state_apply(
  cli_conf: &CliConfig,
  opts: &StateApplyOpts,
) -> IoResult<()> {
  let states =
    parse_state_file_recurr(cli_conf, &opts.state_location, &opts.args).await?;
  if !opts.skip_confirm {
    print_states(&states);
    utils::dialog::confirm("Are you sure to apply this state ?")
      .map_err(|err| err.map_err_context(|| "StateApply"))?;
  }
  for state in &states {
    state_apply(cli_conf, opts, state).await?;
  }
  if opts.follow {
    states
      .iter()
      .map(|state| async {
        state_logs(
          cli_conf,
          &StateLogsOpts {
            state_location: Some(state.root.to_string()),
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

async fn state_logs(
  cli_conf: &CliConfig,
  opts: &StateLogsOpts,
  state_file: &StateRef<Statefile>,
) {
  let client = &cli_conf.client;
  let tail = opts.tail.clone();
  let log_opts = ProcessLogQuery {
    since: opts.since,
    until: opts.until,
    tail,
    timestamps: Some(opts.timestamps),
    follow: Some(opts.follow),
    namespace: state_file.data.namespace.clone(),
    ..Default::default()
  };
  join!(
    log_jobs(
      client,
      state_file.data.jobs.clone().unwrap_or_default(),
      &log_opts
    ),
    log_cargoes(
      client,
      state_file.data.cargoes.clone().unwrap_or_default(),
      &log_opts
    )
  );
}

/// Follow logs of all cargoes in state
async fn exec_state_logs(
  cli_conf: &CliConfig,
  opts: &StateLogsOpts,
) -> IoResult<()> {
  let states =
    parse_state_file_recurr(cli_conf, &opts.state_location, &opts.args).await?;
  states
    .iter()
    .map(|state| state_logs(cli_conf, opts, state))
    .collect::<FuturesUnordered<_>>()
    .collect::<Vec<_>>()
    .await;
  Ok(())
}

async fn state_remove(
  cli_conf: &CliConfig,
  state_file: &StateRef<Statefile>,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let namespace = match &state_file.data.namespace {
    None => "global",
    Some(namespace) => namespace,
  };
  let pg_style = utils::progress::create_spinner_style("red");
  if let Some(jobs) = &state_file.data.jobs {
    for job in jobs {
      let token = format!("job/{}", job.name);
      let pg = utils::progress::create_progress(&token, &pg_style);
      if client.inspect_job(&job.name).await.is_ok() {
        let waiter = wait_process_object(
          &job.name,
          EventActorKind::Job,
          vec![NativeEventAction::Destroy],
          client,
        )
        .await?;
        client.delete_job(&job.name).await?;
        waiter.await??;
      }
      pg.finish();
    }
  }
  if let Some(cargoes) = &state_file.data.cargoes {
    for cargo in cargoes {
      let token = format!("cargo/{}", cargo.name);
      let pg = utils::progress::create_progress(&token, &pg_style);
      if client
        .inspect_cargo(&cargo.name, state_file.data.namespace.as_deref())
        .await
        .is_ok()
      {
        let waiter = wait_process_object(
          &format!("{}.{namespace}", cargo.name),
          EventActorKind::Cargo,
          vec![NativeEventAction::Destroy],
          client,
        )
        .await?;
        client
          .delete_cargo(
            &cargo.name,
            Some(&CargoDeleteQuery {
              namespace: state_file.data.namespace.clone(),
              force: Some(true),
            }),
          )
          .await?;
        waiter.await??;
      }
      pg.finish();
    }
  }
  if let Some(vms) = &state_file.data.virtual_machines {
    for vm in vms {
      let token = format!("vm/{}", vm.name);
      let pg = utils::progress::create_progress(&token, &pg_style);
      if client
        .inspect_vm(&vm.name, state_file.data.namespace.as_deref())
        .await
        .is_ok()
      {
        let waiter = wait_process_object(
          &format!("{}.{namespace}", vm.name),
          EventActorKind::Vm,
          vec![NativeEventAction::Destroy],
          client,
        )
        .await?;
        client
          .delete_vm(&vm.name, state_file.data.namespace.as_deref())
          .await?;
        waiter.await??;
      }
      pg.finish();
    }
  }
  if let Some(resources) = &state_file.data.resources {
    for resource in resources {
      let token = format!("resource/{}", resource.name);
      let pg = utils::progress::create_progress(&token, &pg_style);
      if client.inspect_resource(&resource.name).await.is_ok() {
        client.delete_resource(&resource.name).await?;
      }
      pg.finish();
    }
  }
  if let Some(secrets) = &state_file.data.secrets {
    for secret in secrets {
      let token = format!("secret/{}", secret.name);
      let pg = utils::progress::create_progress(&token, &pg_style);
      if client.inspect_secret(&secret.name).await.is_ok() {
        client.delete_secret(&secret.name).await?;
      }
      pg.finish();
    }
  }
  Ok(())
}

/// Function called when running `nanocl state rm`
async fn exec_state_remove(
  cli_conf: &CliConfig,
  opts: &StateRemoveOpts,
) -> IoResult<()> {
  let state_files =
    parse_state_file_recurr(cli_conf, &opts.state_location, &opts.args).await?;
  if !opts.skip_confirm {
    print_states(&state_files);
    utils::dialog::confirm("Are you sure to remove this state ?")
      .map_err(|err| err.map_err_context(|| "Delete resource"))?;
  }
  for state in &state_files {
    state_remove(cli_conf, state).await?;
  }
  Ok(())
}

/// Function called when running `nanocl state` with correct arguments
pub async fn exec_state(cli_conf: &CliConfig, args: &StateArg) -> IoResult<()> {
  match &args.command {
    StateCommand::Apply(opts) => exec_state_apply(cli_conf, opts).await,
    StateCommand::Remove(opts) => exec_state_remove(cli_conf, opts).await,
    StateCommand::Logs(opts) => exec_state_logs(cli_conf, opts).await,
  }
}
