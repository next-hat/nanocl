use crate::models::*;

use super::{
  http_client::Nanocld,
  error::{NanocldError, is_api_error},
};

impl Nanocld {
  pub async fn create_proxy_template(
    &self,
    item: ProxyTemplatePartial,
  ) -> Result<ProxyTemplatePartial, NanocldError> {
    let mut res = self
      .post(String::from("/proxy/templates"))
      .send_json(&item)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<ProxyTemplatePartial>().await?;
    Ok(item)
  }

  pub async fn delete_proxy_template(
    &self,
    name: String,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .delete(format!("/proxy/templates/{name}", name = name))
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    Ok(())
  }

  pub async fn list_proxy_template(
    &self,
  ) -> Result<Vec<ProxyTemplatePartial>, NanocldError> {
    let mut res = self.get(String::from("/proxy/templates")).send().await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let items = res.json::<Vec<ProxyTemplatePartial>>().await?;
    Ok(items)
  }
}
