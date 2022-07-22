use clap::{Parser, arg_enum};
use tabled::Tabled;
use serde::{Serialize, Deserialize};

use super::{
  client::Nanocld,
  error::{NanocldError, is_api_error},
};

arg_enum! {
  /// Nginx template mode
  /// # Examples
  /// ```
  /// NginxTemplateModes::Http; // For http forward
  /// NginxTemplateModes::Stream; // For low level tcp/udp forward
  /// ```
  #[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
  #[serde(rename_all = "snake_case")]
  pub enum NginxTemplateModes {
    Http,
    Stream,
  }
}

#[derive(Debug, Tabled, Parser, Serialize, Deserialize)]
pub struct NginxTemplatePartial {
  /// Name of template to create
  pub(crate) name: String,
  /// Mode of template http|stream
  pub(crate) mode: NginxTemplateModes,
  /// Content of template
  pub(crate) content: String,
}

impl Nanocld {
  pub async fn create_nginx_template(
    &self,
    item: NginxTemplatePartial,
  ) -> Result<NginxTemplatePartial, NanocldError> {
    let mut res = self
      .post(String::from("/nginx_templates"))
      .send_json(&item)
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let item = res.json::<NginxTemplatePartial>().await?;
    Ok(item)
  }

  pub async fn delete_nginx_template(
    &self,
    name: String,
  ) -> Result<(), NanocldError> {
    let mut res = self
      .delete(format!("/nginx_templates/{name}", name = name))
      .send()
      .await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    Ok(())
  }

  pub async fn list_nginx_template(
    &self,
  ) -> Result<Vec<NginxTemplatePartial>, NanocldError> {
    let mut res = self.get(String::from("/nginx_templates")).send().await?;
    let status = res.status();
    is_api_error(&mut res, &status).await?;
    let items = res.json::<Vec<NginxTemplatePartial>>().await?;
    Ok(items)
  }
}
