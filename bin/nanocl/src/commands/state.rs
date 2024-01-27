use std::sync::{Arc, Mutex};
use std::{fs, str::FromStr};
use std::collections::HashMap;

use futures::StreamExt;
use nanocld_client::stubs::system::ObjPsStatusKind;
use serde_json::{Map, Value};
use clap::{Arg, Command, ArgAction};
use bollard_next::service::HostConfig;

use nanocl_error::{
  io::{IoError, FromIo, IoResult},
  http::HttpError,
};

use nanocld_client::{
  NanocldClient,
  stubs::{
    job::JobPartial,
    statefile::Statefile,
    process::ProcessLogQuery,
    cargo_spec::{CargoSpecPartial, Config},
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
    StateArg, StateCommand, StateApplyOpts, StateRemoveOpts, DisplayFormat,
    StateRef, Context, StateLogsOpts,
  },
};

/// Get Statefile from url and return a StateRef with the raw data and the format
async fn get_from_url(url: &str) -> IoResult<StateRef<Statefile>> {
  let url = if url.starts_with("http") {
    url.to_owned()
  } else {
    format!("http://{url}")
  };
  let reqwest = ntex::http::Client::default();
  let mut res = reqwest.get(&url).send().await.map_err(|err| {
    err.map_err_context(|| "Unable to get Statefile from url")
  })?;
  if res.status().is_redirection() {
    let location = res
      .headers()
      .get("location")
      .ok_or_else(|| IoError::invalid_data("Location", "is not specified"))?
      .to_str()
      .map_err(|err| IoError::invalid_data("Location", &format!("{err}")))?;
    res = reqwest.get(location).send().await.map_err(|err| {
      err.map_err_context(|| "Unable to get Statefile from url")
    })?;
  }
  let data = res
    .body()
    .await
    .map_err(|err| err.map_err_context(|| "Cannot read response from url"))?
    .to_vec();
  let data = std::str::from_utf8(&data).map_err(|err| {
    IoError::invalid_data("From utf8".into(), format!("{err}"))
  })?;
  let ext = url
    .split('.')
    .last()
    .ok_or_else(|| IoError::invalid_data("Statefile", "has no extension"))?;
  let state_ref = utils::state::get_state_ref(ext, data)?;
  Ok(state_ref)
}

/// Read Statefile from file and return a StateRef with the raw data and the format
fn read_from_file<T>(
  path: &std::path::Path,
  format: &DisplayFormat,
) -> IoResult<StateRef<T>>
where
  T: serde::Serialize + serde::de::DeserializeOwned,
{
  let default_format = format.to_string();
  let ext = path
    .extension()
    .unwrap_or(std::ffi::OsStr::new(&default_format))
    .to_str();
  let ext = ext.unwrap_or_default();
  let data = fs::read_to_string(path)?;
  let state_ref = utils::state::get_state_ref::<T>(ext, &data)?;
  Ok(state_ref)
}

/// Hook cargoes binds to replace relative path with absolute path
fn hook_binds(cargo: &CargoSpecPartial) -> IoResult<CargoSpecPartial> {
  let new_cargo = match &cargo.container.host_config {
    None => cargo.clone(),
    Some(host_config) => match &host_config.binds {
      None => cargo.clone(),
      Some(binds) => {
        let mut new_binds = Vec::new();
        for bind in binds {
          let bind_split = bind.split(':').collect::<Vec<&str>>();
          let new_bind = if bind_split.len() == 2 {
            let host_path = bind_split[0];
            if host_path.starts_with('.') {
              let curr_path = std::env::current_dir()?;
              let path = std::path::Path::new(&curr_path)
                .join(std::path::PathBuf::from(host_path));
              let path = path.display().to_string();
              format!("{}:{}", path, bind_split[1])
            } else {
              bind.clone()
            }
          } else {
            bind.clone()
          };
          new_binds.push(new_bind);
        }
        CargoSpecPartial {
          container: Config {
            host_config: Some(HostConfig {
              binds: Some(new_binds),
              ..host_config.clone()
            }),
            ..cargo.container.clone()
          },
          ..cargo.clone()
        }
      }
    },
  };
  Ok(new_cargo)
}

/// Logs existing jobs in the Statefile
async fn log_jobs(
  client: &NanocldClient,
  jobs: Vec<JobPartial>,
  query: &ProcessLogQuery,
) -> IoResult<()> {
  let mut futures = Vec::new();
  for job in jobs {
    let client = client.clone();
    let fut = async move {
      match client.logs_process("job", &job.name, Some(query)).await {
        Err(err) => {
          eprintln!("Cannot attach to job {}: {err}", &job.name);
        }
        Ok(stream) => {
          if let Err(err) = utils::print::logs_process_stream(stream).await {
            eprintln!("{err}");
          }
        }
      }
    };
    futures.push(fut);
  }
  futures::future::join_all(futures).await;
  Ok(())
}

/// Attach to a list of cargoes and print their logs
pub async fn log_cargoes(
  client: &NanocldClient,
  cargoes: Vec<CargoSpecPartial>,
  query: &ProcessLogQuery,
) -> IoResult<()> {
  let mut futures = Vec::new();
  for cargo in cargoes {
    let client = client.clone();
    let fut = async move {
      match client.logs_process("cargo", &cargo.name, Some(query)).await {
        Err(err) => {
          eprintln!("Cannot attach to cargo {}: {err}", &cargo.name);
        }
        Ok(stream) => {
          if let Err(err) = utils::print::logs_process_stream(stream).await {
            eprintln!("{err}");
          }
        }
      }
    };
    futures.push(fut);
  }
  futures::future::join_all(futures).await;
  Ok(())
}

/// Hook cargoes binds to replace relative path with absolute path
fn hook_cargoes(
  cargoes: Vec<CargoSpecPartial>,
) -> IoResult<Vec<CargoSpecPartial>> {
  let mut new_cargoes = Vec::new();
  for cargo in cargoes {
    let new_cargo = hook_binds(&cargo)?;
    new_cargoes.push(new_cargo);
  }
  Ok(new_cargoes)
}

/// Generate a nanocl daemon client based on the api version specified in the Statefile
fn gen_client(
  host: &str,
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
      NanocldClient::connect_to(&url, Some(version.into()))
    }
    api_version if state_ref.data.api_version.starts_with('v') => {
      NanocldClient::connect_to(host, Some(api_version.clone()))
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
      NanocldClient::connect_to(&url, Some(version.into()))
    }
  };
  Ok(client)
}

/// Parse `Args` from a Statefile and ask the user to input their values
fn parse_build_args(
  state_file: &Statefile,
  args: Vec<String>,
) -> IoResult<serde_json::Value> {
  let mut cmd = Command::new("nanocl state args")
    .about("Validate state args")
    .bin_name("nanocl state args --");
  // Add string nanocl state args as first element of args
  let mut args = args;
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
        println!("Boolean {value}");
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
  let str = utils::state::compile(namespace, &object)?;
  Ok(str)
}

/// Inject `Args`, `Envs`, `Config`, `HostGateway` and `Namespaces` to the Statefile
async fn inject_data(
  state_ref: &StateRef<Statefile>,
  args: &serde_json::Value,
  context: &Context,
  client: &NanocldClient,
) -> IoResult<StateRef<Statefile>> {
  let mut envs = std::collections::HashMap::new();
  for (key, value) in std::env::vars_os() {
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
    "Os": std::env::consts::OS,
    "OsFamily": std::env::consts::FAMILY,
    "Config": info.config,
    "HostGateway": info.host_gateway,
    "Namespaces": namespaces,
  });
  let raw = utils::state::compile(&state_ref.raw, &data)?;
  let state_file =
    utils::state::serialize_ext::<Statefile>(&state_ref.format, &raw)?;
  Ok(StateRef {
    raw,
    format: state_ref.format.clone(),
    data: state_file,
  })
}

/// Parse a Statefile from a path or url and return a StateRef with the raw data and the format
async fn parse_state_file(
  path: &Option<String>,
  format: &DisplayFormat,
) -> IoResult<StateRef<Statefile>> {
  if let Some(path) = path {
    if let Ok(path) = std::path::Path::new(&path)
      .canonicalize()
      .map_err(|err| err.map_err_context(|| format!("Statefile {path}")))
    {
      return read_from_file(&path, format);
    }
    return get_from_url(path).await;
  }
  if let Ok(path) = std::path::Path::new("Statefile.yaml").canonicalize() {
    return read_from_file(&path, format);
  }
  if let Ok(path) = std::path::Path::new("Statefile").canonicalize() {
    return read_from_file(&path, format);
  }
  let path = std::path::Path::new("Statefile.yml")
    .canonicalize()
    .map_err(|err| err.map_err_context(|| "Statefile Statefile.yml"))?;
  read_from_file(&path, format)
}

async fn execute_template(
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

/// Function called when running `nanocl state apply`
async fn exec_state_apply(
  cli_conf: &CliConfig,
  opts: &StateApplyOpts,
) -> IoResult<()> {
  let host = &cli_conf.host;
  let format = cli_conf.user_config.display_format.clone();
  let state_ref = parse_state_file(&opts.state_location, &format).await?;
  let client = gen_client(host, &state_ref)?;
  let args = parse_build_args(&state_ref.data, opts.args.clone())?;
  let state_file =
    execute_template(&state_ref, &args, &client, cli_conf).await?;
  let namespace = state_file.data.namespace.clone().unwrap_or("global".into());
  if !opts.skip_confirm {
    println!("{}", state_file.raw);
    utils::dialog::confirm("Are you sure to apply this state ?")
      .map_err(|err| err.map_err_context(|| "StateApply"))?;
  }
  let client_ptr = cli_conf.client.clone();
  let state_ptr = state_file.clone();
  let keys = Arc::new(Mutex::new(
    state_ptr.data.cargoes.unwrap_or_default().into_iter().fold(
      HashMap::new(),
      |mut acc, elem| {
        acc.insert(
          format!(
            "Cargo@{}.{}",
            elem.name,
            state_ptr
              .data
              .namespace
              .clone()
              .unwrap_or("global".to_owned())
          ),
          false,
        );
        acc
      },
    ),
  ));
  let keys_ptr = keys.clone();
  let event_fut = ntex::rt::spawn(async move {
    let mut ev_stream = client_ptr.watch_events().await?;
    while let Some(ev) = ev_stream.next().await {
      let ev = match ev {
        Err(err) => {
          eprintln!("Unable to read event: {err}");
          continue;
        }
        Ok(ev) => ev,
      };
      let Some(actor) = ev.actor else {
        continue;
      };
      let action = NativeEventAction::from_str(&ev.action)
        .map_err(HttpError::bad_request)?;
      let key = actor.key.clone().unwrap_or_default();
      let kind = actor.kind.clone();
      let entry = format!("{kind}@{key}");
      let mut keys = keys_ptr.lock().unwrap();
      if action == NativeEventAction::Start {
        keys.insert(entry, true);
      }
      // check if all keys are set to true
      if keys.values().all(|v| *v) {
        break;
      }
    }
    Ok::<_, HttpError>(())
  });
  let pg_style = utils::progress::create_spinner_style("green");
  if let Some(secrets) = &state_ref.data.secrets {
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
        client.delete_job(&job.name).await?;
      }
      client.create_job(job).await?;
      client.start_process("job", &job.name, None).await?;
      pg.finish();
    }
  }
  if let Some(cargoes) = &state_file.data.cargoes {
    for cargo in cargoes {
      let key = format!("Cargo@{}.{}", cargo.name, namespace);
      let token = format!("cargo/{}", cargo.name);
      let pg = utils::progress::create_progress(&token, &pg_style);
      match client.inspect_cargo(&cargo.name, Some(&namespace)).await {
        Err(_) => {
          client.create_cargo(cargo, Some(&namespace)).await?;
        }
        Ok(inspect) => {
          let cmp: CargoSpecPartial = inspect.spec.into();
          if cmp != *cargo || opts.reload {
            client
              .put_cargo(&cargo.name, cargo, Some(&namespace))
              .await?;
          } else if inspect.status.actual == ObjPsStatusKind::Running {
            keys.lock().unwrap().insert(key, true);
          }
        }
      }
      client
        .start_process("cargo", &cargo.name, Some(&namespace))
        .await?;
      pg.finish();
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
            let cmp: VmSpecPartial = inspect.spec.into();
            if cmp != *vm {
              let update: VmSpecUpdate = vm.clone().into();
              client.patch_vm(&vm.name, &update, Some(&namespace)).await?;
            }
          }
        }
        client
          .start_process("vm", &vm.name, Some(&namespace))
          .await?;
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
  }
  if !keys.lock().unwrap().clone().values().all(|v| *v) {
    event_fut.await??;
  }
  if opts.follow {
    let query = ProcessLogQuery {
      namespace: state_file.data.namespace,
      follow: Some(true),
      ..Default::default()
    };
    if let Some(cargoes) = state_file.data.cargoes {
      log_cargoes(&client, cargoes, &query).await?;
    }
    if let Some(jobs) = state_file.data.jobs {
      log_jobs(&client, jobs, &query).await?;
    }
  }
  Ok(())
}

/// Follow logs of all cargoes in state
async fn exec_state_logs(
  cli_conf: &CliConfig,
  opts: &StateLogsOpts,
) -> IoResult<()> {
  let host = &cli_conf.host;
  let format = cli_conf.user_config.display_format.clone();
  let state_ref = parse_state_file(&opts.state_location, &format).await?;
  let client = gen_client(host, &state_ref)?;
  let args = parse_build_args(&state_ref.data, opts.args.clone())?;
  let state_file =
    execute_template(&state_ref, &args, &client, cli_conf).await?;
  let tail_string = opts.tail.clone().unwrap_or_default();
  let tail = tail_string.as_str();
  let log_opts = ProcessLogQuery {
    since: opts.since,
    until: opts.until,
    tail: if tail.is_empty() {
      None
    } else {
      Some(tail.to_owned())
    },
    timestamps: Some(opts.timestamps),
    follow: Some(opts.follow),
    namespace: state_file.data.namespace,
    ..Default::default()
  };
  if let Some(cargoes) = state_file.data.cargoes {
    log_cargoes(&client, cargoes, &log_opts).await?;
  }
  if let Some(jobs) = state_file.data.jobs {
    log_jobs(&client, jobs, &log_opts).await?;
  }
  Ok(())
}

/// Function called when running `nanocl state rm`
async fn exec_state_remove(
  cli_conf: &CliConfig,
  opts: &StateRemoveOpts,
) -> IoResult<()> {
  let host = &cli_conf.host;
  let format = cli_conf.user_config.display_format.clone();
  let state_ref = parse_state_file(&opts.state_location, &format).await?;
  let client = gen_client(host, &state_ref)?;
  let args = parse_build_args(&state_ref.data, opts.args.clone())?;
  let state_file =
    inject_data(&state_ref, &args, &cli_conf.context, &client).await?;
  if !opts.skip_confirm {
    println!("{}", state_file.raw);
    utils::dialog::confirm("Are you sure to remove this state ?")
      .map_err(|err| err.map_err_context(|| "Delete resource"))?;
  }
  let pg_style = utils::progress::create_spinner_style("red");
  if let Some(jobs) = state_file.data.jobs {
    for job in jobs {
      let token = format!("job/{}", job.name);
      let pg = utils::progress::create_progress(&token, &pg_style);
      if client.inspect_job(&job.name).await.is_ok() {
        client.delete_job(&job.name).await?;
      }
      pg.finish();
    }
  }
  if let Some(cargoes) = state_file.data.cargoes {
    for cargo in cargoes {
      let token = format!("cargo/{}", cargo.name);
      let pg = utils::progress::create_progress(&token, &pg_style);
      if client
        .inspect_cargo(&cargo.name, state_file.data.namespace.as_deref())
        .await
        .is_ok()
      {
        client
          .delete_cargo(
            &cargo.name,
            Some(&CargoDeleteQuery {
              namespace: state_file.data.namespace.clone(),
              force: Some(true),
            }),
          )
          .await?;
      }
      pg.finish();
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
          client
            .delete_vm(&vm.name, state_file.data.namespace.as_deref())
            .await?;
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
