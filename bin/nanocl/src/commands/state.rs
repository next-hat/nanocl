use std::fs;
use std::collections::HashMap;

use ntex::rt;
use futures::StreamExt;
use clap::{Arg, Command, ArgAction};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use indicatif::{MultiProgress, ProgressBar};
use bollard_next::service::HostConfig;

use nanocl_utils::io_error::{IoError, FromIo, IoResult};
use nanocld_client::NanocldClient;
use nanocld_client::stubs::state::StateMeta;
use nanocld_client::stubs::cargo::{OutputKind, CargoLogQuery};
use nanocld_client::stubs::cargo_config::{
  CargoConfigPartial, Config as ContainerConfig,
};

use crate::utils;
use crate::config::CliConfig;
use crate::models::{
  StateArg, StateCommand, StateApplyOpts, StateRemoveOpts, StateBuildArg,
  DisplayFormat, StateRef,
};

use super::cargo_image::exec_cargo_image_pull;

/// ## Get from url
///
/// Get Statefile from url and return a StateRef with the raw data and the format
///
/// ## Arguments
///
/// * [url](str) The url of the Statefile
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](StateRef) The StateRef
///   * [Err](IoError) An error occured
///
async fn get_from_url<T>(url: &str) -> IoResult<StateRef<T>>
where
  T: serde::Serialize + serde::de::DeserializeOwned,
{
  let url = if url.starts_with("http") {
    url.to_owned()
  } else {
    format!("http://{url}")
  };
  let reqwest = ntex::http::Client::default();
  let mut res = reqwest.get(url.to_string()).send().await.map_err(|err| {
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

/// ## Read from file
///
/// Read Statefile from file and return a StateRef with the raw data and the format
///
/// ## Arguments
///
/// * [path](std::path::Path) The path of the Statefile
/// * [format](DisplayFormat) The format of the Statefile
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](StateRef) The StateRef
///   * [Err](IoError) An error occured
///
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

/// ## Download cargo image
///
/// Download cargo image if it's not already downloaded and if the force pull flag is set
///
/// ## Arguments
///
/// * [client](NanocldClient) The client to the daemon
/// * [cargo](CargoConfigPartial) The cargo config
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](IoError) An error occured
///
async fn download_cargo_image(
  client: &NanocldClient,
  cargo: &CargoConfigPartial,
) -> IoResult<()> {
  match &cargo.container.image {
    Some(image) => {
      exec_cargo_image_pull(client, image).await?;
    }
    None => {
      return Err(IoError::invalid_data("Cargo image", "is not specified"))
    }
  }
  Ok(())
}

/// ## Hook binds
///
/// Hook cargoes binds to replace relative path with absolute path
///
/// ## Arguments
///
/// * [cargo](CargoConfigPartial) The cargo config
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](CargoConfigPartial) The cargo config
///   * [Err](IoError) An error occured
///
fn hook_binds(cargo: &CargoConfigPartial) -> IoResult<CargoConfigPartial> {
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
        CargoConfigPartial {
          container: ContainerConfig {
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

/// ## Hook cargoes
///
/// Hook cargoes binds to replace relative path with absolute path
///
/// ## Arguments
///
/// * [cargoes](Vec<CargoConfigPartial>) The cargoes config
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](Vec<CargoConfigPartial>) The cargoes config
///   * [Err](IoError) An error occured
///
fn hook_cargoes(
  cargoes: Vec<CargoConfigPartial>,
) -> IoResult<Vec<CargoConfigPartial>> {
  let mut new_cargoes = Vec::new();
  for cargo in cargoes {
    let new_cargo = hook_binds(&cargo)?;
    new_cargoes.push(new_cargo);
  }
  Ok(new_cargoes)
}

/// ## Attach to cargo
///
/// Attach to a cargo and print its logs
///
/// ## Arguments
///
/// * [client](NanocldClient) The client to the daemon
/// * [cargo](CargoConfigPartial) The cargo config
/// * [namespace](str) The namespace of the cargo
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](Vec<rt::JoinHandle<()>>) The list of futures
///   * [Err](IoError) An error occured
///
async fn attach_to_cargo(
  client: &NanocldClient,
  cargo: CargoConfigPartial,
  namespace: &str,
) -> IoResult<Vec<rt::JoinHandle<()>>> {
  let cargo = match client
    .inspect_cargo(&cargo.name, Some(namespace.to_owned()))
    .await
  {
    Err(err) => {
      eprintln!(
        "Error while inspecting cargo {cargo}: {err}",
        cargo = cargo.name
      );
      return Ok(Vec::default());
    }
    Ok(cargo) => cargo,
  };
  let mut futures = Vec::new();
  for (index, _) in cargo.instances.iter().enumerate() {
    let namespace = namespace.to_owned();
    let name = if index == 0 {
      cargo.name.clone()
    } else {
      format!("{index}-{}", cargo.name)
    };
    let client = client.clone();
    let name = name.clone();
    let fut = rt::spawn(async move {
      let query = CargoLogQuery {
        namespace: Some(namespace),
        follow: Some(true),
        ..Default::default()
      };
      match client.logs_cargo(&name, &query).await {
        Err(err) => {
          eprintln!("Cannot attach to cargo {name}: {err}");
        }
        Ok(mut stream) => {
          while let Some(output) = stream.next().await {
            let output = match output {
              Ok(output) => output,
              Err(e) => {
                eprintln!("Error: {e}");
                break;
              }
            };
            match output.kind {
              OutputKind::StdOut => {
                print!("[{name}]: {}", &output.data);
              }
              OutputKind::StdErr => {
                eprint!("[{name}]: {}", &output.data);
              }
              OutputKind::Console => {
                print!("[{name}]: {}", &output.data);
              }
              _ => {}
            }
          }
        }
      }
    });
    futures.push(fut);
  }
  Ok(futures)
}

/// ## Attach to cargoes
///
/// Attach to a list of cargoes and print their logs
///
/// ## Arguments
///
/// * [client](NanocldClient) The client to the daemon
/// * [cargoes](Vec<CargoConfigPartial>) The list of cargoes
/// * [namespace](str) The namespace of the cargoes
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](IoError) An error occured
///
async fn attach_to_cargoes(
  client: &NanocldClient,
  cargoes: Vec<CargoConfigPartial>,
  namespace: &str,
) -> IoResult<()> {
  let mut futures = Vec::new();
  for cargo in cargoes {
    let more_futures = attach_to_cargo(client, cargo, namespace).await?;
    futures.extend(more_futures);
  }
  futures::future::join_all(futures).await;
  Ok(())
}

/// ## Gen client
///
/// Generate a nanocl daemon client based on the api version specified in the Statefile
///
/// ## Arguments
///
/// * [host](str) The host of the daemon
/// * [meta](StateMeta) The meta of the Statefile
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](NanocldClient) The nanocl daemon client
///   * [Err](IoError) An error occured
///
fn gen_client(host: &str, meta: &StateMeta) -> IoResult<NanocldClient> {
  let client = match meta.api_version.clone() {
    api_version if meta.api_version.starts_with("http") => {
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
      let url = Box::leak(url.into_boxed_str());
      NanocldClient::connect_to(url, Some(version.into()))
    }
    api_version if meta.api_version.starts_with('v') => {
      let url = Box::leak(host.to_owned().into_boxed_str());
      NanocldClient::connect_to(url, Some(api_version))
    }
    _ => {
      let mut paths = meta
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
      let url = Box::leak(url.into_boxed_str());
      NanocldClient::connect_to(url, Some(version.into()))
    }
  };
  Ok(client)
}

/// ## Parse build args
///
/// Parse `Args` from a Statefile and ask the user to input their values
///
/// ## Arguments
///
/// * [yaml](serde_yaml::Value) The yaml value of the Statefile
/// * [args](Vec<String>) The list of arguments
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](HashMap) The list of arguments
///   * [Err](IoError) An error occured
///
fn parse_build_args(
  yaml: &serde_yaml::Value,
  args: Vec<String>,
) -> IoResult<serde_json::Value> {
  let build_args: StateBuildArg = serde_yaml::from_value(yaml.clone())
    .map_err(|err| err.map_err_context(|| "Unable to extract BuildArgs"))?;
  let mut cmd = Command::new("nanocl state args")
    .about("Validate state args")
    .bin_name("nanocl state args --");
  // Add string nanocl state args as first element of args
  let mut args = args;
  args.insert(0, "nanocl state apply --".into());
  for build_arg in build_args.args.clone().unwrap_or_default() {
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
  for build_arg in build_args.args.unwrap_or_default() {
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

/// ## Inject namespace
///
/// Inject `Args` to the namespace value
///
/// ## Arguments
///
/// * [namespace](str) The namespace value
/// * [args](HashMap<String, String>) The list of arguments
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](String) The namespace value
///   * [Err](IoError) An error occured
///
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

/// ## Inject data
///
/// Inject `Args`, `Envs`, `Config`, `HostGateway` and `Namespaces` to the Statefile
///
/// ## Arguments
///
/// * [ext](DisplayFormat) The format of the Statefile
/// * [raw](str) The raw data of the Statefile
/// * [args](HashMap<String, String>) The list of arguments
/// * [client](NanocldClient) The client to the daemon
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](Serialize) The data of the Statefile
///   * [Err](IoError) An error occured
///
async fn inject_data<T>(
  ext: &DisplayFormat,
  raw: &str,
  args: &serde_json::Value,
  client: &NanocldClient,
) -> IoResult<T>
where
  T: Serialize + DeserializeOwned,
{
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
    "Config": info.config,
    "HostGateway": info.host_gateway,
    "Namespaces": namespaces,
  });
  let template = utils::state::compile(raw, &data)?;
  let data = utils::state::serialize_ext(ext, &template)?;
  Ok(data)
}

/// ## Parse state file
///
/// Parse a Statefile from a path or url and return a StateRef with the raw data and the format
///
/// ## Arguments
///
/// * [path](Option<String>) The path or url of the Statefile if empty set to current path + Statefile.yml
/// * [format](DisplayFormat) The format of the Statefile
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](StateRef) The StateRef
///   * [Err](IoError) An error occured
///
async fn parse_state_file<T>(
  path: &Option<String>,
  format: &DisplayFormat,
) -> IoResult<StateRef<T>>
where
  T: serde::Serialize + serde::de::DeserializeOwned,
{
  if let Some(path) = path {
    if let Ok(path) = std::path::Path::new(&path)
      .canonicalize()
      .map_err(|err| err.map_err_context(|| format!("Statefile {path}")))
    {
      return read_from_file(&path, format);
    }
    return get_from_url::<T>(path).await;
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

/// ## Exec state apply
///
/// Function called when running `nanocl state apply`
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
/// * [opts](StateApplyOpts) The state apply options
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](IoError) An error occured
///
async fn exec_state_apply(
  cli_conf: &CliConfig,
  opts: &StateApplyOpts,
) -> IoResult<()> {
  let host = &cli_conf.host;
  let format = cli_conf.user_config.display_format.clone();
  let state_ref = parse_state_file(&opts.state_location, &format).await?;
  let client = gen_client(host, &state_ref.meta)?;
  let args = parse_build_args(&state_ref.data, opts.args.clone())?;
  let mut namespace = String::from("global");
  let mut cargoes = Vec::new();
  let data = match state_ref.meta.kind.as_str() {
    "Deployment" | "Cargo" => {
      namespace = match state_ref.data.get("Namespace") {
        Some(namespace) => serde_yaml::from_value(namespace.clone())
          .map_err(|err| err.map_err_context(|| "Unable to convert to yaml"))?,
        None => "global".to_owned(),
      };
      namespace = inject_namespace(&namespace, &args)?;
      let _ = client.create_namespace(&namespace).await;
      let mut yaml: serde_yaml::Value =
        inject_data(&state_ref.format, &state_ref.raw, &args, &client).await?;
      let current_cargoes: Vec<CargoConfigPartial> = match yaml.get("Cargoes") {
        Some(cargoes) => serde_yaml::from_value(cargoes.clone())
          .map_err(|err| err.map_err_context(|| "Unable to convert to yaml"))?,
        None => Vec::new(),
      };
      let hooked_cargoes = hook_cargoes(current_cargoes)?;
      cargoes = hooked_cargoes.clone();
      yaml["Cargoes"] = serde_yaml::to_value(&hooked_cargoes)
        .map_err(|err| err.map_err_context(|| "Unable to convert to yaml"))?;
      yaml
    }
    _ => inject_data(&state_ref.format, &state_ref.raw, &args, &client).await?,
  };
  if !opts.skip_confirm {
    utils::print::display_format(&state_ref.format, &data)?;
    utils::dialog::confirm("Are you sure to apply this state ?")
      .map_err(|err| err.map_err_context(|| "StateApply"))?;
  }
  for cargo in &cargoes {
    let is_missing = client
      .inspect_cargo_image(&cargo.container.image.clone().unwrap_or_default())
      .await
      .is_err();
    // Download cargoes images
    if is_missing || opts.force_pull {
      if let Err(err) = download_cargo_image(&client, cargo).await {
        eprintln!("{err}");
        if is_missing {
          return Err(err);
        }
      }
    }
  }
  let data = serde_json::to_value(&data).map_err(|err| {
    err.map_err_context(|| "Unable to create json payload for the daemon")
  })?;
  let mut stream = client.apply_state(&data).await?;
  let multiprogress = MultiProgress::new();
  multiprogress.set_move_cursor(false);
  let mut layers: HashMap<String, ProgressBar> = HashMap::new();
  while let Some(res) = stream.next().await {
    let res = res?;
    utils::state::update_progress(&multiprogress, &mut layers, &res.key, &res);
  }
  if opts.follow {
    attach_to_cargoes(&client, cargoes, &namespace).await?;
  }
  Ok(())
}

/// ## Exec state remove
///
/// Function called when running `nanocl state rm`
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
/// * [opts](StateRemoveOpts) The state remove options
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](IoError) An error occured
///
async fn exec_state_remove(
  cli_conf: &CliConfig,
  opts: &StateRemoveOpts,
) -> IoResult<()> {
  let host = &cli_conf.host;
  let format = cli_conf.user_config.display_format.clone();
  let state_ref = parse_state_file(&opts.state_location, &format).await?;
  let client = gen_client(host, &state_ref.meta)?;
  let args = parse_build_args(&state_ref.data, opts.args.clone())?;
  let data: serde_json::Value =
    inject_data(&state_ref.format, &state_ref.raw, &args, &client).await?;
  if !opts.skip_confirm {
    utils::print::display_format(&state_ref.format, &data)?;
    utils::dialog::confirm("Are you sure to remove this state ?")
      .map_err(|err| err.map_err_context(|| "Delete resource"))?;
  }
  let mut stream = client.remove_state(&data).await?;
  let multiprogress = MultiProgress::new();
  multiprogress.set_move_cursor(false);
  let mut layers: HashMap<String, ProgressBar> = HashMap::new();
  while let Some(res) = stream.next().await {
    let res = res?;
    utils::state::update_progress(&multiprogress, &mut layers, &res.key, &res);
  }
  Ok(())
}

/// ## Exec state
///
/// Function called when running `nanocl state` with correct arguments
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
/// * [args](StateArg) The state arguments
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](IoError) An error occured
///
pub async fn exec_state(cli_conf: &CliConfig, args: &StateArg) -> IoResult<()> {
  match &args.command {
    StateCommand::Apply(opts) => exec_state_apply(cli_conf, opts).await,
    StateCommand::Remove(opts) => exec_state_remove(cli_conf, opts).await,
  }
}
