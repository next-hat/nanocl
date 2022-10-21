use crate::models::*;

use super::{
  http_client::Nanocld,
  error::{NanocldError, is_api_error},
};

impl Nanocld {
  pub async fn list_cargo(
    &self,
    namespace: Option<String>,
  ) -> Result<Vec<CargoItem>, NanocldError> {
    let mut res = self
      .get(String::from("/cargoes"))
      .query(&GenericNamespaceQuery { namespace })
      .unwrap()
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let items = res.json::<Vec<CargoItem>>().await?;

    Ok(items)
  }

  pub async fn create_cargo(
    &self,
    item: &CargoPartial,
    namespace: Option<String>,
  ) -> Result<CargoItem, NanocldError> {
    let mut res = self
      .post(String::from("/cargoes"))
      .query(&GenericNamespaceQuery { namespace })
      .unwrap()
      .send_json(item)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<CargoItem>().await?;

    Ok(item)
  }

  pub async fn delete_cargo(
    &self,
    cargo_name: &str,
    namespace: Option<String>,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .delete(format!("/cargoes/{name}", name = cargo_name))
      .query(&GenericNamespaceQuery { namespace })
      .unwrap()
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }

  pub async fn count_cargo(
    &self,
    namespace: Option<String>,
  ) -> Result<PgGenericCount, NanocldError> {
    let mut res = self
      .get(String::from("/cargoes/count"))
      .query(&GenericNamespaceQuery { namespace })
      .unwrap()
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let count = res.json::<PgGenericCount>().await?;
    Ok(count)
  }

  pub async fn inspect_cargo(
    &self,
    name: &str,
    namespace: Option<String>,
  ) -> Result<CargoItemWithRelation, NanocldError> {
    let mut res = self
      .get(format!("/cargoes/{name}/inspect", name = name))
      .query(&GenericNamespaceQuery { namespace })
      .unwrap()
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<CargoItemWithRelation>().await?;

    Ok(item)
  }

  pub async fn update_cargo(
    &self,
    name: &str,
    namespace: Option<String>,
    payload: &CargoPatchPartial,
  ) -> Result<CargoItem, NanocldError> {
    let mut res = self
      .patch(format!("/cargoes/{name}"))
      .query(&GenericNamespaceQuery { namespace })
      .unwrap()
      .send_json(payload)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    let cargo = res.json::<CargoItem>().await?;

    Ok(cargo)
  }

  pub async fn delete_cargo_instance(
    &self,
    name: &str,
    cluster_name: &str,
    namespace: Option<String>,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .delete(format!("/clusters/{cluster_name}/cargoes/{name}"))
      .query(&GenericNamespaceQuery { namespace })
      .unwrap()
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;

    Ok(())
  }
}
