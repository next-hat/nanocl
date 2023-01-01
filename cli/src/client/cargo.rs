use nanocl_models::cargo::{Cargo, CargoPartial};

use crate::models::GenericNamespaceQuery;

use super::{
  http_client::Nanocld,
  error::{NanocldError, is_api_error},
};

impl Nanocld {
  pub async fn create_cargo(
    &self,
    item: &CargoPartial,
    namespace: Option<String>,
  ) -> Result<Cargo, NanocldError> {
    let mut res = self
      .post(String::from("/cargoes"))
      .query(&GenericNamespaceQuery { namespace })?
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
  ) -> Result<(), NanocldError> {
    let mut res = self
      .delete(format!("/cargoes/{}", name))
      .query(&GenericNamespaceQuery { namespace })?
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }
}
