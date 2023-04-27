use std::fs;

use ntex::rt;
use clap::{Command, Arg};
use dialoguer::Confirm;
use dialoguer::theme::ColorfulTheme;
use futures::StreamExt;

use bollard_next::service::HostConfig;

use nanocl_utils::io_error::{IoResult, FromIo, IoError};

use nanocld_client::NanocldClient;
use nanocld_client::stubs::cargo::{OutputKind, CargoLogQuery};
use nanocld_client::stubs::cargo_config::{
  CargoConfigPartial, Config as ContainerConfig,
};
use nanocld_client::stubs::state::{
  StateConfig, StateDeployment, StateCargo, StateStream,
};

use crate::utils;
use crate::models::{StateArgs, StateCommands, StateOpts, StateBuildArgs};

use super::cargo_image::exec_cargo_image_create;

async fn get_from_url(
  url: String,
) -> IoResult<(StateConfig, serde_yaml::Value)> {
  let reqwest = ntex::http::Client::default();
  let data = reqwest
    .get(url.to_string())
    .send()
    .await
    .map_err(|err| err.map_err_context(|| "Unable to get StateFile from url"))?
    .body()
    .await
    .map_err(|err| err.map_err_context(|| "Cannot read response from url"))?
    .to_vec();
  let data = std::str::from_utf8(&data).map_err(|err| {
    IoError::invalid_data("From utf8".into(), format!("{err}"))
  })?;
  let meta = utils::state::get_file_meta(&String::from(data))?;
  let yaml: serde_yaml::Value = serde_yaml::from_str(data).map_err(|err| {
    err.map_err_context(|| "Unable to convert response into yaml")
  })?;
  Ok((meta, yaml))
}

async fn get_from_file(
  path: &str,
) -> IoResult<(StateConfig, serde_yaml::Value)> {
  let mut file_path = std::env::current_dir()?;
  file_path.push(path);
  let data = fs::read_to_string(file_path)?;
  let meta = utils::state::get_file_meta(&String::from(&data))?;
  let yaml: serde_yaml::Value = serde_yaml::from_str(&data)
    .map_err(|err| err.map_err_context(|| "Unable to parse yaml"))?;
  Ok((meta, yaml))
}

async fn download_cargo_image(
  client: &NanocldClient,
  cargo: &CargoConfigPartial,
) -> IoResult<()> {
  match &cargo.container.image {
    Some(image) => {
      exec_cargo_image_create(client, image).await?;
    }
    None => {
      return Err(IoError::invalid_data("Cargo image", "is not specified"))
    }
  }
  Ok(())
}

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

async fn hook_cargoes(
  client: &NanocldClient,
  cargoes: Vec<CargoConfigPartial>,
) -> IoResult<Vec<CargoConfigPartial>> {
  let mut new_cargoes = Vec::new();
  for cargo in cargoes {
    if client
      .inspect_cargo_image(&cargo.container.image.clone().unwrap_or_default())
      .await
      .is_err()
    {
      download_cargo_image(client, &cargo).await?;
    }

    let new_cargo = hook_binds(&cargo)?;
    new_cargoes.push(new_cargo);
  }
  Ok(new_cargoes)
}

fn inject_meta(
  meta: StateConfig,
  mut yml: serde_yaml::Value,
) -> serde_yaml::Value {
  yml["ApiVersion"] = serde_yaml::Value::String(meta.api_version);
  yml["Type"] = serde_yaml::Value::String(meta.r#type);
  yml
}

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
      format!("{}-{}", cargo.name, index)
    };
    let client = client.clone();
    let name = name.clone();
    let fut = rt::spawn(async move {
      let query = CargoLogQuery::of_namespace(namespace.to_owned());
      match client.logs_cargo(&name, &query).await {
        Err(err) => {
          eprintln!(
            "Cannot attach to cargo {cargo}: {err}",
            cargo = name,
            err = err
          );
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

fn gen_client(meta: &StateConfig) -> IoResult<NanocldClient> {
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
        .ok_or(IoError::not_fount("Version", "is not specified"))?;
      paths.remove(paths.len() - 1);
      let url = paths.join("/");
      NanocldClient::connect_with_url(&url, version)
    }
    api_version if meta.api_version.starts_with('v') => {
      NanocldClient::connect_with_unix_version(&api_version)
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
        .ok_or(IoError::not_fount("Version", "is not specified"))?;
      paths.remove(paths.len() - 1);
      let url = paths.join("/");
      NanocldClient::connect_with_url(&format!("https://{url}"), version)
    }
  };
  Ok(client)
}

fn inject_build_args(
  yaml: serde_yaml::Value,
  args: Vec<String>,
) -> IoResult<serde_yaml::Value> {
  let build_args: StateBuildArgs = serde_yaml::from_value(yaml.clone())
    .map_err(|err| err.map_err_context(|| "Unable to extract BuildArgs"))?;

  if build_args.args.is_none() {
    return Ok(yaml);
  }

  let mut cmd = Command::new("nanocl state args")
    .about("Validate state args")
    .bin_name("nanocl state args --");
  // Add string nanocl state args as fist element of args
  let mut args = args;
  args.insert(0, "nanocl state apply --".into());
  for build_arg in build_args.args.clone().unwrap_or_default() {
    let name = build_arg.name.to_owned();
    let arg: &'static str = Box::leak(name.into_boxed_str());
    let mut cmd_arg = Arg::new(arg).long(arg);
    match build_arg.default {
      Some(default) => {
        let default_value: &'static str = Box::leak(default.into_boxed_str());
        cmd_arg = cmd_arg.default_value(default_value);
      }
      None => {
        cmd_arg = cmd_arg.required(true);
      }
    }
    cmd = cmd.arg(cmd_arg);
  }
  let matches = cmd.get_matches_from(args);

  let mut args = std::collections::HashMap::new();

  for build_arg in build_args.args.unwrap_or_default() {
    let name = build_arg.name.to_owned();
    let arg: &'static str = Box::leak(name.to_owned().into_boxed_str());
    match build_arg.r#type.as_str() {
      "String" => {
        let value =
          matches.get_one::<String>(arg).ok_or(IoError::invalid_data(
            "BuildArgs".into(),
            format!("argument {arg} is missing"),
          ))?;
        args.insert(name, value.to_owned());
      }
      "Number" => {
        let value =
          matches.get_one::<i64>(arg).ok_or(IoError::invalid_data(
            "BuildArgs".into(),
            format!("argument {arg} is missing"),
          ))?;
        args.insert(name, format!("{value}"));
      }
      _ => {
        return Err(IoError::invalid_data(
          "StateFile".into(),
          format!("unknown type {}", build_arg.r#type),
        ))
      }
    }
  }

  let mut envs = std::collections::HashMap::new();
  for (key, value) in std::env::vars_os() {
    let key = key.to_string_lossy().to_string();
    let value = value.to_string_lossy().to_string();
    envs.insert(key, value);
  }

  let str = serde_yaml::to_string(&yaml)
    .map_err(|err| err.map_err_context(|| "Unable to convert to yaml"))?;
  let template = mustache::compile_str(&str).map_err(|err| {
    IoError::invalid_data("Template".into(), format!("{err}"))
  })?;
  let str = template
    .render_to_string(&serde_json::json!({
      "Args": args,
      "Envs": envs,
    }))
    .map_err(|err| {
      IoError::invalid_data("Template".into(), format!("{err}"))
    })?;
  let yaml: serde_yaml::Value = serde_yaml::from_str(&str)
    .map_err(|err| err.map_err_context(|| "Unable to convert to yaml"))?;
  Ok(yaml)
}

async fn exec_state_apply(opts: &StateOpts) -> IoResult<()> {
  let (meta, yaml) = match utils::url::parse_url(&opts.file_path) {
    Ok(url) => get_from_url(url).await?,
    Err(_) => get_from_file(&opts.file_path).await?,
  };

  let client = gen_client(&meta)?;

  let yaml = inject_build_args(yaml.clone(), opts.args.clone())?;

  let mut namespace = String::from("default");
  let mut cargoes = Vec::new();
  let yaml = match meta.r#type.as_str() {
    "Cargo" => {
      let mut data = serde_yaml::from_value::<StateCargo>(yaml)
        .map_err(|err| err.map_err_context(|| "Unable to parse StateCargo"))?;
      namespace = data.namespace.clone().unwrap_or(namespace);
      cargoes = hook_cargoes(&client, data.cargoes).await?;
      data.cargoes = cargoes.clone();
      let yml = serde_yaml::to_value(data).map_err(|err| {
        err.map_err_context(|| "Unable to convert to yaml value")
      })?;
      inject_meta(meta, yml)
    }
    "Deployment" => {
      let mut data =
        serde_yaml::from_value::<StateDeployment>(yaml).map_err(|err| {
          err.map_err_context(|| "Unable to parse StateDeployment")
        })?;
      namespace = data.namespace.clone().unwrap_or(namespace);
      cargoes = hook_cargoes(&client, data.cargoes.unwrap_or_default()).await?;
      data.cargoes = Some(cargoes.clone());
      let yml = serde_yaml::to_value(data).map_err(|err| {
        err.map_err_context(|| "Unable to convert to yaml value")
      })?;
      inject_meta(meta, yml)
    }
    _ => yaml,
  };
  let data = serde_json::to_value(&yaml)
    .map_err(|err| err.map_err_context(|| "Unable to convert to yaml value"))?;
  let _ = utils::print::print_yml(&yaml);
  if !opts.skip_confirm {
    let result = Confirm::with_theme(&ColorfulTheme::default())
      .with_prompt("Are you sure to apply this new state ?")
      .default(false)
      .interact();
    match result {
      Ok(true) => {}
      _ => {
        return Err(IoError::interupted("State apply", "interupted by user"))
      }
    }
  }
  let mut stream = client.apply_state(&data).await?;

  while let Some(res) = stream.next().await {
    let res = res?;
    match res {
      StateStream::Error(err) => eprintln!("{err}"),
      StateStream::Msg(msg) => println!("{msg}"),
    }
  }

  if opts.attach {
    attach_to_cargoes(&client, cargoes, &namespace).await?;
  }
  Ok(())
}

async fn exec_state_revert(opts: &StateOpts) -> IoResult<()> {
  let (meta, yaml) = match utils::url::parse_url(&opts.file_path) {
    Ok(url) => get_from_url(url).await?,
    Err(_) => get_from_file(&opts.file_path).await?,
  };

  let client = gen_client(&meta)?;

  let yaml = inject_build_args(yaml.clone(), opts.args.clone())?;
  let data = serde_json::to_value(&yaml)
    .map_err(|err| err.map_err_context(|| "Unable to parse yaml"))?;
  let _ = utils::print::print_yml(&yaml);
  if !opts.skip_confirm {
    let result = Confirm::with_theme(&ColorfulTheme::default())
      .with_prompt("Are you sure to revert this state ?")
      .default(false)
      .interact();
    match result {
      Ok(true) => {}
      _ => {
        return Err(IoError::interupted("StateRevert", "interupted by user"))
      }
    }
  }
  let mut stream = client.revert_state(&data).await?;
  while let Some(res) = stream.next().await {
    let res = res?;
    match res {
      StateStream::Error(err) => eprintln!("{err}"),
      StateStream::Msg(msg) => println!("{msg}"),
    }
  }
  Ok(())
}

pub async fn exec_state(args: &StateArgs) -> IoResult<()> {
  match &args.commands {
    StateCommands::Apply(opts) => exec_state_apply(opts).await,
    StateCommands::Revert(opts) => exec_state_revert(opts).await,
  }
}
