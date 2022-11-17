use futures::{TryStreamExt, StreamExt};

use crate::models::*;

use super::{
  http_client::Nanocld,
  error::{NanocldError, is_api_error},
};

impl Nanocld {
  pub async fn list_cargo_instance(
    &self,
    options: &ListCargoInstanceOptions,
  ) -> Result<Vec<CargoInstanceSummary>, NanocldError> {
    let mut res = self
      .get(String::from("/cargoes/instances"))
      .query(options)?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    let data = res.json::<Vec<CargoInstanceSummary>>().await?;

    Ok(data)
  }

  pub async fn create_cargo_instance_exec(
    &self,
    name: &str,
    config: CargoInstanceExecQuery,
  ) -> Result<ExecItem, NanocldError> {
    let mut res = self
      .post(format!("/cargoes/instances/{}/exec", name))
      .send_json(&config)
      .await?;
    let status = res.status();

    is_api_error(&mut res, &status).await?;

    let exec = res.json::<ExecItem>().await?;

    // let test: Result<ExecItem, serde_json::Error> =
    //   serde_json::from_value(exec);
    Ok(exec)
  }

  pub async fn start_cargo_instance_exec(
    &self,
    id: &str,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .post(format!("/cargoes/instances/exec/{}/start", &id))
      .send()
      .await?;
    let status = res.status();

    is_api_error(&mut res, &status).await?;

    let mut stream = res.into_stream();

    while let Some(output) = stream.next().await {
      match output {
        Err(err) => {
          eprintln!("{err}");
        }
        Ok(output) => {
          let Ok(output) = String::from_utf8(output.to_vec()) else {
            eprintln!("Unable to convert current stream into string");
            break;
          };
          print!("{}", output);
        }
      }
    }
    Ok(())
  }
}
