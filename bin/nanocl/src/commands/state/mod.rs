use std::{
  collections::HashMap,
  env::{consts, vars_os},
  fs,
  path::{Path, PathBuf},
};

use async_recursion::async_recursion;
use clap::{Arg, ArgAction, Command};
use futures::{StreamExt, stream::FuturesOrdered};
use serde_json::{Map, Value};
use url::Url;

use nanocl_error::io::{FromIo, IoError, IoResult};
use nanocld_client::{
  ConnectOpts, NanocldClient,
  stubs::{
    cargo_spec::CargoSpec,
    statefile::{Statefile, StatefileArgKind, StatefileArgsValue, SubState},
  },
};

use crate::{
  config::CliConfig,
  models::{
    Context, DisplayFormat, StateArg, StateCommand, StateRef, StateRoot,
  },
  utils,
};

mod apply;
mod diff;
mod logs;
mod man;
mod remove;
mod render;
mod status;

use apply::exec_state_apply;
use logs::exec_state_logs;
use man::execute_man;
use remove::exec_state_remove;
use render::exec_state_render;

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

/// Hook cargoes binds to replace relative path with absolute path
fn hook_cargoes(cargoes: Vec<CargoSpec>) -> IoResult<Vec<CargoSpec>> {
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
        ssl: cli_conf.client.ssl.clone(),
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

pub enum ArgParseMode {
  Apply,
  Diff,
  Status,
  Remove,
  Logs,
}

impl std::fmt::Display for ArgParseMode {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let data = match self {
      ArgParseMode::Apply => "apply",
      ArgParseMode::Diff => "diff",
      ArgParseMode::Status => "status",
      ArgParseMode::Remove => "remove",
      ArgParseMode::Logs => "logs",
    };
    write!(f, "{data}")
  }
}

/// Parse `Args` from a Statefile and ask the user to input their values
fn parse_build_args(
  state_file: &Statefile,
  mode: ArgParseMode,
  args: &[String],
  json: bool,
) -> IoResult<serde_json::Value> {
  let metadata = state_file.clone().metadata.unwrap_or_default();
  let about = match metadata.about {
    Some(about) => about,
    None => "Validate state args".to_owned(),
  };
  let name = format!("nanocl state {mode} -s statefile --");
  let name: &'static str = Box::leak(name.into_boxed_str());
  let mut cmd = Command::new(name).about(about).bin_name(name);
  if let Some(long_about) = metadata.long_about {
    cmd = cmd.long_about(long_about);
  }
  // Add string nanocl state args as first element of args
  let mut args = args.to_owned();
  args.insert(0, "nanocl state apply --".into());
  for build_arg in state_file.args.clone().unwrap_or_default() {
    let name = build_arg.name.to_owned();
    let arg: &'static str = Box::leak(name.into_boxed_str());
    let mut cmd_arg = Arg::new(arg).long(arg);
    if let Some(description) = &build_arg.description {
      let description = description.replace('\n', "");
      cmd_arg = cmd_arg.help(description);
    }
    if build_arg.kind == StatefileArgKind::Boolean {
      cmd_arg = cmd_arg.required(false).action(ArgAction::SetTrue);
    } else {
      cmd_arg = cmd_arg.action(ArgAction::Set).required(build_arg.required);
      if build_arg.multiple {
        cmd_arg = cmd_arg.num_args(1..).action(ArgAction::Append);
      } else {
        cmd_arg = cmd_arg.num_args(1);
      }
    }
    if let Some(default) = build_arg.default
      && build_arg.kind != StatefileArgKind::Boolean
    {
      match default {
        StatefileArgsValue::String(s) => {
          let leaked: &'static str = Box::leak(s.into_boxed_str());
          cmd_arg = cmd_arg.default_value(leaked);
        }
        StatefileArgsValue::Number(n) => {
          let s = n.to_string();
          let leaked: &'static str = Box::leak(s.into_boxed_str());
          cmd_arg = cmd_arg.default_value(leaked);
        }
        StatefileArgsValue::MultipleString(v) => {
          if build_arg.multiple {
            let leaked: Vec<&'static str> = v
              .into_iter()
              .map(|s| Box::leak(s.into_boxed_str()) as &'static str)
              .collect();
            cmd_arg = cmd_arg.default_values(leaked);
          } else if let Some(first) = v.into_iter().next() {
            let leaked: &'static str = Box::leak(first.into_boxed_str());
            cmd_arg = cmd_arg.default_value(leaked);
          }
        }
        StatefileArgsValue::MultipleNumber(v) => {
          if build_arg.multiple {
            let leaked: Vec<&'static str> = v
              .into_iter()
              .map(|n| {
                Box::leak(n.to_string().into_boxed_str()) as &'static str
              })
              .collect();
            cmd_arg = cmd_arg.default_values(leaked);
          } else if let Some(first) = v.into_iter().next() {
            let leaked: &'static str =
              Box::leak(first.to_string().into_boxed_str());
            cmd_arg = cmd_arg.default_value(leaked);
          }
        }
        StatefileArgsValue::Boolean(_) => { /* ignored for non-boolean kind */ }
      }
    }
    cmd = cmd.arg(cmd_arg);
  }
  let matches = if json {
    cmd.try_get_matches_from(args).map_err(|err| {
      IoError::invalid_input("Statefile arguments", err.to_string().as_str())
    })?
  } else {
    cmd.get_matches_from(args)
  };
  let mut args = Map::new();
  for build_arg in state_file.args.clone().unwrap_or_default() {
    let name = build_arg.name.to_owned();
    let arg: &'static str = Box::leak(name.to_owned().into_boxed_str());
    match build_arg.kind {
      StatefileArgKind::String => {
        if build_arg.multiple {
          let values = matches.get_many::<String>(arg);
          match values {
            None if build_arg.required => {
              return Err(IoError::invalid_data(
                "BuildArg".into(),
                format!("argument {arg} is missing"),
              ));
            }
            Some(vals) => {
              let arr = vals
                .map(|v| Value::String(v.to_owned()))
                .collect::<Vec<_>>();
              args.insert(name, Value::Array(arr));
            }
            _ => {}
          }
        } else {
          let value = matches.get_one::<String>(arg);
          match value {
            None if build_arg.required => {
              return Err(IoError::invalid_data(
                "BuildArg".into(),
                format!("argument {arg} is missing"),
              ));
            }
            Some(value) => {
              args.insert(name, Value::String(value.to_owned()));
            }
            _ => {}
          }
        }
      }
      StatefileArgKind::Boolean => {
        let value = matches.get_flag(&name);
        args.insert(name, Value::Bool(value));
      }
      StatefileArgKind::Number => {
        if build_arg.multiple {
          let values = matches.get_many::<String>(arg);
          match values {
            None if build_arg.required => {
              return Err(IoError::invalid_data(
                "BuildArg".into(),
                format!("argument {arg} is missing"),
              ));
            }
            Some(vals) => {
              let mut arr = Vec::new();
              for v in vals {
                let parsed = v.parse::<f64>().map_err(|err| {
                  IoError::invalid_data(
                    "BuildArg".into(),
                    format!(
                      "argument {arg} contains a non-number value: {err}"
                    ),
                  )
                })?;
                let num =
                  serde_json::Number::from_f64(parsed).ok_or_else(|| {
                    IoError::invalid_data(
                      "BuildArg".into(),
                      format!("argument {arg} contains an invalid number"),
                    )
                  })?;
                arr.push(Value::Number(num));
              }
              args.insert(name, Value::Array(arr));
            }
            _ => {}
          }
        } else {
          let value = matches.get_one::<String>(arg);
          match value {
            None if build_arg.required => {
              return Err(IoError::invalid_data(
                "BuildArg".into(),
                format!("argument {arg} is missing"),
              ));
            }
            Some(value) => {
              let parsed = value.parse::<f64>().map_err(|err| {
                IoError::invalid_data(
                  "BuildArg".into(),
                  format!("argument {arg} is not a number: {err}"),
                )
              })?;
              let num =
                serde_json::Number::from_f64(parsed).ok_or_else(|| {
                  IoError::invalid_data(
                    "BuildArg".into(),
                    format!("argument {arg} is not a valid number"),
                  )
                })?;
              args.insert(name, Value::Number(num));
            }
            _ => {}
          }
        }
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
    "Args": args.clone(),
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
    "Args": args.clone(),
    "Envs": envs.clone(),
    "Context": context.clone(),
    "Os": consts::OS,
    "OsFamily": consts::FAMILY,
    "Config": info.config,
    "HostGateway": info.host_gateway,
    "Namespaces": namespaces.clone(),
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
  read_only: bool,
) -> IoResult<StateRef<Statefile>> {
  let mut namespace = match &state_ref.data.namespace {
    Some(namespace) => namespace.clone(),
    None => "global".to_owned(),
  };
  namespace = inject_namespace(&namespace, args)?;
  if !read_only && client.inspect_namespace(&namespace).await.is_err() {
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

fn substate_default_args(
  statefile: &StateRef<Statefile>,
  compiled_values: &mut Map<String, Value>,
) -> IoResult<()> {
  let Some(args) = &statefile.data.args else {
    return Ok(());
  };
  for arg in args {
    match &arg.default {
      None => {}
      Some(value) => match arg.kind {
        StatefileArgKind::String => {
          compiled_values.insert(arg.name.clone(), serde_json::json!(value));
        }
        StatefileArgKind::Number => {
          compiled_values.insert(arg.name.clone(), serde_json::json!(value));
        }
        StatefileArgKind::Boolean => {
          compiled_values.insert(arg.name.clone(), serde_json::json!(value));
        }
      },
    }
  }
  Ok(())
}

#[async_recursion(?Send)]
async fn parse_state_file_recurr(
  cli_conf: &CliConfig,
  state_file: &StateRef<Statefile>,
  args: &Value,
  read_only: bool,
) -> IoResult<Vec<StateRef<Statefile>>> {
  let client = gen_client(cli_conf, state_file)?;
  let state_file =
    render_template(state_file, args, &client, cli_conf, read_only).await?;
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
        let mut compiled_values = match sub_state_args {
          Some(sub_state_args) => {
            sub_state_args
              .iter()
              .try_fold(Map::new(), |mut init, arg| {
                match &arg.value {
                  StatefileArgsValue::String(value) => {
                    init.insert(arg.name.clone(), Value::String(value.clone()));
                  }
                  StatefileArgsValue::Number(value) => {
                    init.insert(arg.name.clone(), serde_json::json!(value));
                  }
                  StatefileArgsValue::Boolean(value) => {
                    init.insert(arg.name.clone(), Value::Bool(*value));
                  }
                  StatefileArgsValue::MultipleNumber(values) => {
                    init.insert(
                      arg.name.clone(),
                      serde_json::json!(values.clone()),
                    );
                  }
                  StatefileArgsValue::MultipleString(values) => {
                    init.insert(
                      arg.name.clone(),
                      serde_json::json!(values.clone()),
                    );
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
          substate_default_args(&state_file, &mut compiled_values)?;
          return parse_state_file_recurr(
            cli_conf,
            &state_file,
            &Value::Object(compiled_values),
            read_only,
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
        substate_default_args(&state_file, &mut compiled_values)?;
        parse_state_file_recurr(
          cli_conf,
          &state_file,
          &Value::Object(compiled_values),
          read_only,
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

fn get_nanocl_group(state_file: &StateRef<Statefile>) -> String {
  match &state_file.data.group {
    Some(group) => group.to_owned(),
    None => state_file.location.to_owned(),
  }
}

fn state_item_count(state: &StateRef<Statefile>) -> u64 {
  let data = &state.data;
  (data.secrets.as_ref().map_or(0, Vec::len)
    + data.jobs.as_ref().map_or(0, Vec::len)
    + data.cargoes.as_ref().map_or(0, Vec::len)
    + data.virtual_machines.as_ref().map_or(0, Vec::len)
    + data.resources.as_ref().map_or(0, Vec::len)) as u64
}

fn print_states(states: &[StateRef<Statefile>]) {
  let raw = states.iter().fold(String::new(), |init, state| {
    format!("{init}{}\n", state.raw.trim())
  });
  println!("{raw}");
}

/// Function called when running `nanocl state` with correct arguments
pub async fn exec_state(cli_conf: &CliConfig, args: &StateArg) -> IoResult<()> {
  match &args.command {
    StateCommand::Man(opts) => execute_man(&opts.source).await,
    StateCommand::Apply(opts) => exec_state_apply(cli_conf, opts).await,
    StateCommand::Diff(opts) => diff::exec_state_diff(cli_conf, opts).await,
    StateCommand::Status(opts) => {
      status::exec_state_status(cli_conf, opts).await
    }
    StateCommand::Render(opts) => exec_state_render(cli_conf, opts).await,
    StateCommand::Remove(opts) => exec_state_remove(cli_conf, opts).await,
    StateCommand::Logs(opts) => exec_state_logs(cli_conf, opts).await,
  }
}

#[cfg(test)]
mod tests {
  use super::{ArgParseMode, parse_build_args};
  use nanocld_client::stubs::statefile::Statefile;

  #[test]
  fn json_state_args_return_errors_instead_of_exiting() {
    let state: Statefile =
      serde_json::from_value(serde_json::json!({"ApiVersion": "v0.18"}))
        .unwrap();
    for mode in [ArgParseMode::Apply, ArgParseMode::Remove] {
      let error = parse_build_args(&state, mode, &["--unknown".into()], true)
        .unwrap_err();
      assert_eq!(error.inner.kind(), std::io::ErrorKind::InvalidInput);
    }
    let help =
      parse_build_args(&state, ArgParseMode::Apply, &["--help".into()], true)
        .unwrap_err();
    assert_eq!(help.inner.kind(), std::io::ErrorKind::InvalidInput);
    assert!(
      parse_build_args(&state, ArgParseMode::Apply, &[], true)
        .unwrap()
        .as_object()
        .unwrap()
        .is_empty()
    );
  }
}
