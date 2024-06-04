use std::{
  fs,
  time::Duration,
  collections::HashMap,
  path::{Path, PathBuf},
  env::{consts, vars_os},
};

use url::Url;
use serde_json::{Map, Value};
use clap::{Arg, Command, ArgAction};
use async_recursion::async_recursion;
use futures::{
  join, StreamExt,
  stream::{FuturesOrdered, FuturesUnordered},
};

use nanocl_error::io::{IoError, FromIo, IoResult};

use nanocld_client::{
  stubs::{
    generic::{GenericClause, GenericFilter, GenericFilterNsp},
    process::Process,
    statefile::{StatefileArgKind, SubState, SubStateValue},
    system::{EventActorKind, ObjPsStatusKind},
  },
  ConnectOpts,
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
    system::NativeEventAction,
  },
};

use crate::{
  config::CliConfig,
  models::{
    CargoArg, Context, DisplayFormat, GenericDefaultOpts,
    GenericRemoveForceOpts, GenericRemoveOpts, JobArg, ResourceArg, SecretArg,
    StateApplyOpts, StateArg, StateCommand, StateLogsOpts, StateRef,
    StateRemoveOpts, StateRoot, VmArg,
  },
  utils,
};

use super::GenericCommandRm;

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
    utils::state::get_state_ref(&ext, &url, &data, StateRoot::Url(root))?;
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
    path.to_str().unwrap(),
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
  // TODO: find a better way to wait for job process to start
  // sleep for 2 seconds for job process to start
  ntex::time::sleep(Duration::from_secs(2)).await;
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
      })?
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
      })?
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
      })?
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
        if build_arg.kind != StatefileArgKind::Boolean {
          let default_value: &'static str = Box::leak(default.into_boxed_str());
          cmd_arg = cmd_arg.default_value(default_value);
        }
      }
      None => {
        if build_arg.kind == StatefileArgKind::Boolean {
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
    match build_arg.kind {
      StatefileArgKind::String => {
        let value =
          matches.get_one::<String>(arg).ok_or(IoError::invalid_data(
            "BuildArg".into(),
            format!("argument {arg} is missing"),
          ))?;
        args.insert(name, Value::String(value.to_owned()));
      }
      StatefileArgKind::Boolean => {
        let value = matches.get_flag(&name);
        args.insert(name, Value::Bool(value));
      }
      StatefileArgKind::Number => {
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

fn generate_envs() -> HashMap<String, String> {
  vars_os().fold(HashMap::new(), |mut init, (key, value)| {
    let key = key.to_string_lossy().to_string();
    let value = value.to_string_lossy().to_string();
    init.insert(key, value);
    init
  })
}

/// Inject `Args`, `Envs`, `Config`, `HostGateway` and `Namespaces` to the Statefile
async fn inject_data(
  state_ref: &StateRef<Statefile>,
  args: &serde_json::Value,
  context: &Context,
  client: &NanocldClient,
) -> IoResult<StateRef<Statefile>> {
  let envs = generate_envs();
  let info = client.info().await?;
  let namespaces = client.list_namespace(None).await?.into_iter().fold(
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
    location: state_ref.location.clone(),
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

#[async_recursion(?Send)]
async fn parse_state_file_recurr(
  cli_conf: &CliConfig,
  state_file: &StateRef<Statefile>,
  args: &Value,
) -> IoResult<Vec<StateRef<Statefile>>> {
  let client = gen_client(cli_conf, state_file)?;
  let state_file = render_template(state_file, args, &client, cli_conf).await?;
  let sub_states = state_file.data.sub_states.clone().unwrap_or_default();
  let parsed_sub_states = sub_states
    .iter()
    .map(|sub_state| {
      let root = state_file.root.clone();
      let parent_location = state_file.location.clone();
      async move {
        let (sub_state_path, sub_state_args) = match sub_state {
          SubState::Path(path) => (path, None),
          SubState::Definition(sub_state) => {
            (&sub_state.path, sub_state.args.clone())
          }
        };
        let compiled_values = match sub_state_args {
          Some(sub_state_args) => {
            sub_state_args
              .iter()
              .try_fold(Map::new(), |mut init, arg| {
                match &arg.value {
                  SubStateValue::String(value) => {
                    init.insert(arg.name.clone(), Value::String(value.clone()));
                  }
                  SubStateValue::Number(value) => {
                    init.insert(arg.name.clone(), serde_json::json!(value));
                  }
                  SubStateValue::Boolean(value) => {
                    init.insert(arg.name.clone(), Value::Bool(*value));
                  }
                }
                Ok::<_, IoError>(init)
              })?
          }
          None => Map::new(),
        };
        if sub_state_path.starts_with("http") {
          let state_file = read_state_file(
            &Some(sub_state_path.clone()),
            &cli_conf.user_config.display_format,
          )
          .await?;
          return parse_state_file_recurr(
            cli_conf,
            &state_file,
            &Value::Object(compiled_values),
          )
          .await;
        }
        let full_sub_state_path = match root {
          StateRoot::Url(url) => Url::parse(&url)
            .expect("Can't parse root url")
            .join(sub_state_path)
            .expect("Can't join url")
            .to_string(),
          StateRoot::File(path) => {
            let current = PathBuf::from(parent_location)
              .canonicalize()
              .map_err(|err| err.map_err_context(|| "Statefile location"))?;
            let full_path = path.join(sub_state_path);
            if current == full_path {
              return Err(IoError::invalid_data(
                "Statefile",
                "Cannot include itself",
              ));
            }
            full_path
              .to_str()
              .expect("Can't convert full path to string")
              .to_owned()
          }
          StateRoot::None => sub_state_path.clone(),
        };
        let state_file = read_state_file(
          &Some(full_sub_state_path.clone()),
          &cli_conf.user_config.display_format,
        )
        .await?;
        parse_state_file_recurr(
          cli_conf,
          &state_file,
          &Value::Object(compiled_values),
        )
        .await
      }
    })
    .collect::<FuturesOrdered<_>>()
    .collect::<Vec<_>>()
    .await
    .into_iter()
    .collect::<Result<Vec<_>, _>>()?;
  let mut states = vec![state_file.clone()];
  // TODO: check if we need to reverse the order of parsed_sub_states
  // parsed_sub_states.reverse();
  states.append(&mut parsed_sub_states.into_iter().flatten().collect());
  states.reverse();
  Ok(states)
}

fn insert_nanocl_group(
  metadata: Option<serde_json::Value>,
  group: &str,
) -> serde_json::Value {
  match metadata {
    Some(mut metadata) => {
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

fn get_nanocl_group(state_file: &StateRef<Statefile>) -> String {
  match &state_file.data.group {
    Some(group) => group.to_owned(),
    None => state_file.location.to_owned(),
  }
}

async fn state_apply(
  cli_conf: &CliConfig,
  opts: &StateApplyOpts,
  state_file: &StateRef<Statefile>,
) -> IoResult<()> {
  let client = &cli_conf.client;
  let namespace = state_file.data.namespace.clone().unwrap_or("global".into());
  let nanocl_group = get_nanocl_group(state_file);
  if let Some(secrets) = &state_file.data.secrets {
    for mut secret in secrets.iter().cloned() {
      let token = format!("secret/{}", secret.name);
      let pg_style = utils::progress::create_spinner_style(&token, "green");
      let pg = utils::progress::create_progress("(submitting)", &pg_style);
      let metadata =
        insert_nanocl_group(secret.metadata.clone(), &nanocl_group);
      secret.metadata = Some(metadata);
      match client.inspect_secret(&secret.name).await {
        Err(_) => {
          client.create_secret(&secret).await?;
          pg.set_message("(created)");
        }
        Ok(inspect) => {
          let cmp: SecretPartial = inspect.into();
          if cmp != secret {
            let update: SecretUpdate = secret.clone().into();
            client.patch_secret(&secret.name, &update).await?;
            pg.set_message("(updated)");
          } else {
            pg.finish_with_message("(unchanged)");
            continue;
          }
        }
      }
      pg.finish_with_message("(done)");
    }
  }
  if let Some(jobs) = &state_file.data.jobs {
    for mut job in jobs.iter().cloned() {
      let token = format!("job/{}", job.name);
      let pg_style = utils::progress::create_spinner_style(&token, "green");
      let pg = utils::progress::create_progress("(submitting)", &pg_style);
      let metadata = insert_nanocl_group(job.metadata.clone(), &nanocl_group);
      job.metadata = Some(metadata);
      if client.inspect_job(&job.name).await.is_ok() {
        pg.set_message("(clearing)");
        let waiter = utils::process::wait_process_state(
          &job.name,
          EventActorKind::Job,
          vec![NativeEventAction::Destroy],
          client,
        )
        .await?;
        client.delete_job(&job.name).await?;
        waiter.await??;
        pg.set_message("(cleared)");
      }
      client.create_job(&job).await?;
      pg.set_message("(created)");
      let waiter = utils::process::wait_process_state(
        &job.name,
        EventActorKind::Job,
        vec![NativeEventAction::Start],
        client,
      )
      .await?;
      pg.set_message("(starting)");
      client.start_process("job", &job.name, None).await?;
      waiter.await??;
      pg.finish_with_message("(running)");
    }
  }
  if let Some(cargoes) = &state_file.data.cargoes {
    for mut cargo in cargoes.iter().cloned() {
      let token = format!("cargo/{}", cargo.name);
      let pg_style = utils::progress::create_spinner_style(&token, "green");
      let pg = utils::progress::create_progress("(submitting)", &pg_style);
      let metadata = insert_nanocl_group(cargo.metadata.clone(), &nanocl_group);
      cargo.metadata = Some(metadata);
      match client.inspect_cargo(&cargo.name, Some(&namespace)).await {
        Err(_) => {
          client.create_cargo(&cargo, Some(&namespace)).await?;
          pg.set_message("(created)");
        }
        Ok(inspect) => {
          let cmp: CargoSpecPartial = inspect.spec.into();
          if (cmp != cargo) || opts.reload {
            client
              .put_cargo(&cargo.name, &cargo, Some(&namespace))
              .await?;
            pg.set_message("(updated)");
          } else if inspect.status.actual == ObjPsStatusKind::Start {
            pg.finish_with_message("(unchanged)");
            continue;
          }
        }
      }
      pg.set_message("(starting)");
      let waiter = utils::process::wait_process_state(
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
      pg.finish_with_message("(running)");
    }
  }
  if let Some(vms) = &state_file.data.virtual_machines {
    for mut vm in vms.iter().cloned() {
      let token = format!("vm/{}", vm.name);
      let pg_style = utils::progress::create_spinner_style(&token, "green");
      let pg = utils::progress::create_progress("(submitting)", &pg_style);
      let metadata = insert_nanocl_group(vm.metadata.clone(), &nanocl_group);
      vm.metadata = Some(metadata);
      match client.inspect_vm(&vm.name, Some(&namespace)).await {
        Err(_) => {
          client.create_vm(&vm, Some(&namespace)).await?;
          pg.set_message("(created)");
        }
        Ok(inspect) => {
          let cmp: VmSpecPartial = inspect.spec.into();
          if (cmp != vm) || opts.reload {
            let update: VmSpecUpdate = vm.clone().into();
            client.patch_vm(&vm.name, &update, Some(&namespace)).await?;
            pg.set_message("(updated)");
          } else if inspect.status.actual == ObjPsStatusKind::Start {
            pg.finish_with_message("(unchanged)");
            continue;
          }
        }
      }
      pg.set_message("(starting)");
      let waiter = utils::process::wait_process_state(
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
      pg.finish_with_message("(running)");
    }
  }
  if let Some(resources) = &state_file.data.resources {
    for mut resource in resources.iter().cloned() {
      let token = format!("resource/{}", resource.name);
      let pg_style = utils::progress::create_spinner_style(&token, "green");
      let pg = utils::progress::create_progress("(submitting)", &pg_style);
      let metadata =
        insert_nanocl_group(resource.metadata.clone(), &nanocl_group);
      resource.metadata = Some(metadata);
      match client.inspect_resource(&resource.name).await {
        Err(_) => {
          client.create_resource(&resource).await?;
          pg.set_message("(created)");
        }
        Ok(inspect) => {
          let cmp: ResourcePartial = inspect.into();
          if (cmp != resource) || opts.reload {
            let update: ResourceUpdate = resource.clone().into();
            client.put_resource(&resource.name, &update).await?;
            pg.set_message("(updated)");
          } else {
            pg.finish_with_message("(unchanged)");
            continue;
          }
        }
      }
      pg.finish_with_message("(done)");
    }
  }
  Ok(())
}

fn print_states(states: &[StateRef<Statefile>]) {
  let raw = states.iter().fold(String::new(), |init, state| {
    format!("{init}{}\n", state.raw.trim())
  });
  println!("{raw}");
}

async fn remove_orphans(
  cli_conf: &CliConfig,
  state: &StateRef<Statefile>,
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
  let old_cargoes: Vec<CargoSpecPartial> = cli_conf
    .client
    .list_cargo(Some(&GenericFilterNsp {
      filter: Some(filter.clone()),
      namespace: state.data.namespace.clone(),
    }))
    .await?
    .iter()
    .map(|cargo| cargo.spec.clone().into())
    .collect();
  let old_vms: Vec<VmSpecPartial> = cli_conf
    .client
    .list_vm(Some(&GenericFilterNsp {
      filter: Some(filter.clone()),
      namespace: state.data.namespace.clone(),
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
  state_remove(cli_conf, &old_state).await?;
  Ok(())
}

/// Function called when running `nanocl state apply`
async fn exec_state_apply(
  cli_conf: &CliConfig,
  opts: &StateApplyOpts,
) -> IoResult<()> {
  let format = cli_conf.user_config.display_format.clone();
  let state_file = read_state_file(&opts.state_location, &format).await?;
  let args = parse_build_args(&state_file.data, &opts.args)?;
  let states = parse_state_file_recurr(cli_conf, &state_file, &args).await?;
  if !opts.skip_confirm {
    print_states(&states);
    utils::dialog::confirm("Are you sure to apply this state ?")
      .map_err(|err| err.map_err_context(|| "StateApply"))?;
  }
  for state in &states {
    if opts.remove_orphans {
      remove_orphans(cli_conf, state).await?;
    }
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
  let format = cli_conf.user_config.display_format.clone();
  let state_file = read_state_file(&opts.state_location, &format).await?;
  let args = parse_build_args(&state_file.data, &opts.args)?;
  let states = parse_state_file_recurr(cli_conf, &state_file, &args).await?;
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
  let mut gen_rm_opts = GenericRemoveOpts::<GenericDefaultOpts> {
    keys: Vec::default(),
    skip_confirm: true,
    others: GenericDefaultOpts,
  };
  if let Some(jobs) = &state_file.data.jobs {
    gen_rm_opts.keys = jobs.iter().map(|job| job.name.clone()).collect();
    let _ = JobArg::exec_rm(client, &gen_rm_opts, None).await;
  }
  if let Some(cargoes) = &state_file.data.cargoes {
    let opts = GenericRemoveOpts::<GenericRemoveForceOpts> {
      keys: cargoes.iter().map(|cargo| cargo.name.clone()).collect(),
      skip_confirm: true,
      others: GenericRemoveForceOpts { force: true },
    };
    let _ = CargoArg::exec_rm(client, &opts, Some(namespace.to_owned())).await;
  }
  if let Some(vms) = &state_file.data.virtual_machines {
    gen_rm_opts.keys = vms.iter().map(|vm| vm.name.clone()).collect();
    let _ =
      VmArg::exec_rm(client, &gen_rm_opts, Some(namespace.to_owned())).await;
  }
  if let Some(resources) = &state_file.data.resources {
    gen_rm_opts.keys = resources
      .iter()
      .map(|resource| resource.name.clone())
      .collect();
    let _ = ResourceArg::exec_rm(client, &gen_rm_opts, None).await;
  }
  if let Some(secrets) = &state_file.data.secrets {
    gen_rm_opts.keys =
      secrets.iter().map(|secret| secret.name.clone()).collect();
    let _ = SecretArg::exec_rm(client, &gen_rm_opts, None).await;
  }
  Ok(())
}

/// Function called when running `nanocl state rm`
async fn exec_state_remove(
  cli_conf: &CliConfig,
  opts: &StateRemoveOpts,
) -> IoResult<()> {
  let format = cli_conf.user_config.display_format.clone();
  let state_file = read_state_file(&opts.state_location, &format).await?;
  let args = parse_build_args(&state_file.data, &opts.args)?;
  let state_files =
    parse_state_file_recurr(cli_conf, &state_file, &args).await?;
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
