use std::fs;

use clap::{Command, Arg};
use dialoguer::Confirm;
use dialoguer::theme::ColorfulTheme;
use futures::StreamExt;

use bollard_next::service::HostConfig;

use nanocld_client::NanocldClient;
use nanocld_client::stubs::cargo::CargoOutputKind;
use nanocld_client::stubs::cargo_config::{CargoConfigPartial, ContainerConfig};
use nanocld_client::stubs::state::{StateConfig, StateDeployment, StateCargo};
use ntex::rt::{self, JoinHandle};

use crate::utils;
use crate::error::CliError;
use crate::models::{StateArgs, StateCommands, StateOpts, StateBuildArgs};
use crate::utils::print::print_yml;

use super::cargo_image::exec_create_cargo_image;

async fn get_from_url(
  url: url::Url,
) -> Result<(StateConfig, serde_yaml::Value), CliError> {
  let reqwest = ntex::http::Client::default();
  let data = reqwest
    .get(url.to_string())
    .send()
    .await
    .map_err(|err| CliError::Custom {
      msg: format!("Cannot fetch state file from url: {err}"),
    })?
    .body()
    .await
    .map_err(|err| CliError::Custom {
      msg: format!("Cannot fetch state file from url: {err}"),
    })?
    .to_vec();
  let data = std::str::from_utf8(&data).map_err(|err| CliError::Custom {
    msg: format!("Cannot fetch state file from url: {err}"),
  })?;
  let meta = utils::state::get_file_meta(&String::from(data))?;
  let yaml: serde_yaml::Value = serde_yaml::from_str(data)?;
  Ok((meta, yaml))
}

async fn get_from_file(
  path: &str,
) -> Result<(StateConfig, serde_yaml::Value), CliError> {
  let mut file_path = std::env::current_dir()?;
  file_path.push(path);
  let data = fs::read_to_string(file_path)?;
  let meta = utils::state::get_file_meta(&String::from(&data))?;
  let yaml: serde_yaml::Value = serde_yaml::from_str(&data)?;
  Ok((meta, yaml))
}

async fn download_cargo_image(
  client: &NanocldClient,
  cargo: &CargoConfigPartial,
) -> Result<(), CliError> {
  match &cargo.container.image {
    Some(image) => {
      exec_create_cargo_image(client, image).await?;
    }
    None => {
      return Err(CliError::Custom {
        msg: format!("Cargo image is not specified for {}", cargo.name),
      });
    }
  }
  Ok(())
}

fn hook_binds(
  cargo: &CargoConfigPartial,
) -> Result<CargoConfigPartial, CliError> {
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
            if host_path.starts_with("./") {
              let curr_path = std::env::current_dir()?;
              let path = std::path::Path::new(&curr_path)
                .join(std::path::PathBuf::from(host_path.replace("./", "")));
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
          container: ContainerConfig::<String> {
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
) -> Result<Vec<CargoConfigPartial>, CliError> {
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
) -> Result<Vec<JoinHandle<()>>, CliError> {
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
  for (index, _) in cargo.containers.iter().enumerate() {
    let namespace = namespace.to_owned();
    let name = if index == 0 {
      cargo.name.clone()
    } else {
      format!("{}-{}", cargo.name, index)
    };
    let client = client.clone();
    let name = name.clone();
    let fut = rt::spawn(async move {
      match client.logs_cargo(&name, Some(namespace.to_owned())).await {
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
              CargoOutputKind::StdOut => {
                print!("[{name}]: {}", &output.data);
              }
              CargoOutputKind::StdErr => {
                eprint!("[{name}]: {}", &output.data);
              }
              CargoOutputKind::Console => {
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
) -> Result<(), CliError> {
  let mut futures = Vec::new();
  for cargo in cargoes {
    let more_futures = attach_to_cargo(client, cargo, namespace).await?;
    futures.extend(more_futures);
  }
  futures::future::join_all(futures).await;
  Ok(())
}

fn gen_client(meta: &StateConfig) -> Result<NanocldClient, CliError> {
  let client = match meta.api_version.clone() {
    api_version if meta.api_version.starts_with("http") => {
      let mut paths = api_version
        .split('/')
        .map(|e| e.to_owned())
        .collect::<Vec<String>>();
      // extract and remove last item of paths
      let path_ptr = paths.clone();
      let version = path_ptr.last().ok_or(CliError::Custom {
        msg: "Please add version to the path".into(),
      })?;
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
      let version = path_ptr.last().ok_or(CliError::Custom {
        msg: "Please add version to the path".into(),
      })?;
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
) -> Result<serde_yaml::Value, CliError> {
  let build_args: StateBuildArgs = serde_yaml::from_value(yaml.clone())?;

  let mut cmd = Command::new("nanocl state args")
    .about("Validate state args")
    .bin_name("nanocl state args");
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
        let value = matches.get_one::<String>(arg).ok_or(CliError::Custom {
          msg: format!("Missing argument {arg}"),
        })?;
        args.insert(name, value.to_owned());
      }
      "Number" => {
        let value = matches.get_one::<i64>(arg).ok_or(CliError::Custom {
          msg: format!("Missing argument {arg}"),
        })?;
        args.insert(name, format!("{value}"));
      }
      _ => {
        return Err(CliError::Custom {
          msg: format!("Unknown type {type}", type = build_arg.r#type),
        })
      }
    }
  }

  let mut envs = std::collections::HashMap::new();
  for (key, value) in std::env::vars_os() {
    let key = key.to_string_lossy().to_string();
    let value = value.to_string_lossy().to_string();
    envs.insert(key, value);
  }

  let str = serde_yaml::to_string(&yaml)?;
  let template =
    mustache::compile_str(&str).map_err(|err| CliError::Custom {
      msg: format!("Cannot compile mustache template: {err}"),
    })?;
  let str = template
    .render_to_string(&serde_json::json!({
      "Args": args,
      "Envs": envs,
    }))
    .map_err(|err| CliError::Custom {
      msg: format!("Cannot render mustache template: {err}"),
    })?;
  let yaml: serde_yaml::Value = serde_yaml::from_str(&str)?;
  Ok(yaml)
}

async fn exec_apply(opts: &StateOpts) -> Result<(), CliError> {
  let (meta, yaml) = match url::Url::parse(&opts.file_path) {
    Ok(url) => get_from_url(url).await?,
    Err(_) => get_from_file(&opts.file_path).await?,
  };

  let client = gen_client(&meta)?;

  let yaml = inject_build_args(yaml.clone(), opts.args.clone())?;

  let mut namespace = String::from("default");
  let mut cargoes = Vec::new();
  let yaml = match meta.r#type.as_str() {
    "Cargo" => {
      let mut data = serde_yaml::from_value::<StateCargo>(yaml)?;
      namespace = data.namespace.clone().unwrap_or(namespace);
      cargoes = hook_cargoes(&client, data.cargoes).await?;
      data.cargoes = cargoes.clone();
      let yml = serde_yaml::to_value(data)?;
      inject_meta(meta, yml)
    }
    "Deployment" => {
      let mut data = serde_yaml::from_value::<StateDeployment>(yaml)?;
      namespace = data.namespace.clone().unwrap_or(namespace);
      cargoes = hook_cargoes(&client, data.cargoes.unwrap_or_default()).await?;
      data.cargoes = Some(cargoes.clone());
      let yml = serde_yaml::to_value(data)?;
      inject_meta(meta, yml)
    }
    _ => yaml,
  };
  let data = serde_json::to_value(&yaml)?;
  let _ = print_yml(&yaml);
  if !opts.skip_confirm {
    let result = Confirm::with_theme(&ColorfulTheme::default())
      .with_prompt("Are you sure to apply this new state ?")
      .default(false)
      .interact();
    match result {
      Ok(true) => {}
      _ => {
        return Err(CliError::Custom {
          msg: "Aborted".into(),
        })
      }
    }
  }
  client.apply_state(&data).await?;
  if opts.attach {
    attach_to_cargoes(&client, cargoes, &namespace).await?;
  }
  Ok(())
}

async fn exec_revert(opts: &StateOpts) -> Result<(), CliError> {
  let (meta, yaml) = match url::Url::parse(&opts.file_path) {
    Ok(url) => get_from_url(url).await?,
    Err(_) => get_from_file(&opts.file_path).await?,
  };

  let client = gen_client(&meta)?;

  let yaml = inject_build_args(yaml.clone(), opts.args.clone())?;
  let data = serde_json::to_value(&yaml)?;
  let _ = print_yml(&yaml);
  if !opts.skip_confirm {
    let result = Confirm::with_theme(&ColorfulTheme::default())
      .with_prompt("Are you sure to revert this state ?")
      .default(false)
      .interact();
    match result {
      Ok(true) => {}
      _ => {
        return Err(CliError::Custom {
          msg: "Aborted".into(),
        })
      }
    }
  }
  client.revert_state(&data).await?;
  Ok(())
}

pub async fn exec_state(args: &StateArgs) -> Result<(), CliError> {
  match &args.commands {
    StateCommands::Apply(opts) => exec_apply(opts).await,
    StateCommands::Revert(opts) => exec_revert(opts).await,
  }
}
