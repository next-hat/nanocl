use bollard_next::{
  secret::Network,
  network::{CreateNetworkOptions, InspectNetworkOptions},
};

use nanocl_error::http::HttpResult;

use crate::models::SystemState;

pub async fn inspect_network(
  pk: &str,
  state: &SystemState,
) -> HttpResult<Network> {
  match state
    .inner
    .docker_api
    .inspect_network(pk, None::<InspectNetworkOptions<String>>)
    .await
  {
    Err(err) => match err {
      bollard_next::errors::Error::DockerResponseServerError {
        status_code,
        message: _,
      } => {
        if status_code != 404 {
          return Err(err.into());
        }
        state
          .inner
          .docker_api
          .create_network(CreateNetworkOptions {
            name: pk.to_owned(),
            driver: String::from("bridge"),
            ..Default::default()
          })
          .await?;
        Ok(
          state
            .inner
            .docker_api
            .inspect_network(pk, None::<InspectNetworkOptions<String>>)
            .await?,
        )
      }
      _ => Err(err.into()),
    },
    Ok(network) => Ok(network),
  }
}
