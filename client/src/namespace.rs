use nanocl_models::namespace::Namespace;

use super::{
  http_client::NanoclClient,
  error::{NanoclClientError, is_api_error},
};

impl NanoclClient {
  pub async fn list_namespace(
    &self,
  ) -> Result<Vec<Namespace>, NanoclClientError> {
    let mut res = self.get(String::from("/namespaces")).send().await?;

    println!("res: {:?}", &res);
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let items = res.json::<Vec<Namespace>>().await?;

    println!("items: {:?}", &items);
    Ok(items)
  }

  pub async fn create_namespace(
    &self,
    name: &str,
  ) -> Result<Namespace, NanoclClientError> {
    let new_item = Namespace { name: name.into() };
    let mut res = self
      .post(String::from("/namespaces"))
      .send_json(&new_item)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<Namespace>().await?;

    Ok(item)
  }

  pub async fn inspect_namespace(
    &self,
    name: &str,
  ) -> Result<Namespace, NanoclClientError> {
    let mut res = self
      .get(format!("/namespaces/{name}/inspect", name = name))
      .send()
      .await?;

    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<Namespace>().await?;

    Ok(item)
  }
}
