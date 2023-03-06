use std::fs;

use futures::StreamExt;

use bollard_next::service::HostConfig;

use nanocld_client::NanocldClient;
use nanocld_client::stubs::cargo::CargoOutputKind;
use nanocld_client::stubs::cargo_config::{CargoConfigPartial, ContainerConfig};
use nanocld_client::stubs::state::{StateConfig, StateDeployment, StateCargo};
use ntex::rt::{self, JoinHandle};

use crate::utils;
use crate::error::CliError;
use crate::models::{StateArgs, StateCommands, StateOpts};

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
        msg: format!(
          "Image is not defined for cargo {cargo}",
          cargo = cargo.name
        ),
      })
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

async fn exec_apply(
  client: &NanocldClient,
  opts: &StateOpts,
) -> Result<(), CliError> {
  let (meta, yaml) = match url::Url::parse(&opts.file_path) {
    Ok(url) => get_from_url(url).await?,
    Err(_) => get_from_file(&opts.file_path).await?,
  };
  let mut namespace = String::from("default");
  let mut cargoes = Vec::new();
  let yml = match meta.r#type.as_str() {
    "Cargo" => {
      let mut data = serde_yaml::from_value::<StateCargo>(yaml)?;
      namespace = data.namespace.clone().unwrap_or(namespace);
      cargoes = hook_cargoes(client, data.cargoes).await?;
      data.cargoes = cargoes.clone();
      let yml = serde_yaml::to_value(data)?;
      inject_meta(meta, yml)
    }
    "Deployment" => {
      let mut data = serde_yaml::from_value::<StateDeployment>(yaml)?;
      namespace = data.namespace.clone().unwrap_or(namespace);
      cargoes = hook_cargoes(client, data.cargoes.unwrap_or_default()).await?;
      data.cargoes = Some(cargoes.clone());
      let yml = serde_yaml::to_value(data)?;
      inject_meta(meta, yml)
    }
    _ => yaml,
  };
  let data = serde_json::to_value(yml)?;
  client.apply_state(&data).await?;
  if opts.attach {
    attach_to_cargoes(client, cargoes, &namespace).await?;
  }
  Ok(())
}

async fn exec_revert(
  client: &NanocldClient,
  opts: &StateOpts,
) -> Result<(), CliError> {
  let (_meta, yaml) = match url::Url::parse(&opts.file_path) {
    Ok(url) => get_from_url(url).await?,
    Err(_) => get_from_file(&opts.file_path).await?,
  };
  let data = serde_json::to_value(yaml)?;
  client.revert_state(&data).await?;
  Ok(())
}

pub async fn exec_state(
  client: &NanocldClient,
  args: &StateArgs,
) -> Result<(), CliError> {
  match &args.commands {
    StateCommands::Apply(opts) => exec_apply(client, opts).await,
    StateCommands::Revert(opts) => exec_revert(client, opts).await,
  }
}
