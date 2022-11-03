use std::collections::HashMap;
use std::str::FromStr;

use bollard::container::{CreateContainerOptions, Config, StartContainerOptions};
use bollard::image::CreateImageOptions;
use bollard::service::{HostConfig, RestartPolicy, RestartPolicyNameEnum};
use futures::StreamExt;
use indicatif::{ProgressStyle, ProgressBar};
use ntex::http::StatusCode;
use std::default::Default;
use bollard::image::ImportImageOptions;
use bollard::errors::Error;
use tokio::fs::File;
use tokio_util::codec;

use crate::client::error::ApiError;
use crate::models::SetupArgs;
use crate::utils::file;
use crate::config::read_daemon_config_file;

use super::errors::CliError;

pub async fn exec_setup(args: &SetupArgs) -> Result<(), CliError> {
  let config = read_daemon_config_file(&String::from("/etc/nanocl"))?;
  match &args.host {
    // Host is empty perform local installation
    None => {
      // Connect to docker daemon
      let docker_api = bollard::Docker::connect_with_unix(
        &config.docker_host,
        120,
        bollard::API_DEFAULT_VERSION,
      )?;
      let options = Some(CreateImageOptions {
        from_image: "cockroachdb/cockroach:v21.2.17",
        ..Default::default()
      });

      let mut stream = docker_api.create_image(options, None, None);
      let style = ProgressStyle::default_spinner();
      let pg = ProgressBar::new(0);
      pg.set_style(style);
      while let Some(chunk) = stream.next().await {
        if let Err(err) = chunk {
          eprintln!("Error while downloading store {err}");
          std::process::exit(1);
        }
        pg.tick();
      }
      pg.finish_and_clear();

      let daemon_url = "https://github.com/nxthat/nanocld/releases/download/v0.1.6/nanocl-daemon.0.1.6.tar.gz";

      let daemon_url =
        url::Url::from_str(daemon_url).map_err(|err| ApiError {
          status: StatusCode::INTERNAL_SERVER_ERROR,
          msg: format!("{err}"),
        })?;

      let mut download_response = file::download(&daemon_url, "/tmp").await?;
      let style = ProgressStyle::default_spinner();
      let pg = ProgressBar::new(0);
      pg.set_style(style);
      while let Some(chunk) = download_response.stream.next().await {
        if let Err(err) = chunk {
          eprintln!("Error while downloading daemon {err}");
          std::process::exit(1);
        }
        pg.tick();
      }
      pg.finish_and_clear();

      let file = File::open(format!("/tmp/{}", &download_response.path))
        .await
        .unwrap();

      let byte_stream = codec::FramedRead::new(file, codec::BytesCodec::new())
        .map(|r| {
          let bytes = r.unwrap().freeze();
          Ok::<_, Error>(bytes)
        });
      let body = hyper::Body::wrap_stream(byte_stream);
      let mut stream = docker_api.import_image(
        ImportImageOptions {
          ..Default::default()
        },
        body,
        None,
      );

      while let Some(_) = stream.next().await {}

      let options = Some(CreateContainerOptions {
        name: "system-nanocl-daemon",
      });

      let host_config = HostConfig {
        binds: Some(vec![
          format!("{}:/run/nanocl/docker.sock", &config.docker_host),
          String::from("/run/nanocl:/run/nanocl"),
          String::from("/var/lib/nanocl:/var/lib/nanocl"),
        ]),
        restart_policy: Some(RestartPolicy {
          name: Some(RestartPolicyNameEnum::UNLESS_STOPPED),
          maximum_retry_count: None,
        }),
        network_mode: Some(String::from("host")),
        ..Default::default()
      };

      let mut labels = HashMap::new();
      labels.insert("namespace", "system");
      labels.insert("cluster", "system-nano");
      labels.insert("cargo", "system-daemon");

      let config = Config {
        image: Some("nanocl-daemon:0.1.6"),
        labels: Some(labels),
        host_config: Some(host_config),
        ..Default::default()
      };

      docker_api.create_container(options, config).await?;

      docker_api
        .start_container(
          "system-nanocl-daemon",
          None::<StartContainerOptions<String>>,
        )
        .await?;
    }
    // Host is exists perform remote installation
    Some(_host) => {
      todo!("Remote installation is not available yet.");
    }
  }
  Ok(())
}
