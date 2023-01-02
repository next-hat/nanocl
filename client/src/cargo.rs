use nanocl_models::generic::GenericNspQuery;
use nanocl_models::cargo::{Cargo, CargoPartial};

use super::{
  http_client::NanoclClient,
  error::{NanoclClientError, is_api_error},
};

impl NanoclClient {
  pub async fn create_cargo(
    &self,
    item: &CargoPartial,
    namespace: Option<String>,
  ) -> Result<Cargo, NanoclClientError> {
    let mut res = self
      .post(String::from("/cargoes"))
      .query(&GenericNspQuery { namespace })?
      .send_json(item)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<Cargo>().await?;

    Ok(item)
  }

  pub async fn delete_cargo(
    &self,
    name: String,
    namespace: Option<String>,
  ) -> Result<(), NanoclClientError> {
    let mut res = self
      .delete(format!("/cargoes/{}", name))
      .query(&GenericNspQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }
}
