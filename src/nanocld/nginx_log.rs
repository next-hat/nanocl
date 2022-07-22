use serde::{Serialize, Deserialize};
use chrono::{DateTime, FixedOffset};
use futures::{TryStreamExt, StreamExt};

use super::{
  client::Nanocld,
  error::{NanocldError, is_api_error},
};

#[derive(Debug, Serialize, Deserialize)]
pub struct NginxLogItem {
  pub(crate) key: String,
  pub(crate) date_gmt: DateTime<FixedOffset>,
  pub(crate) uri: String,
  pub(crate) host: String,
  pub(crate) remote_addr: String,
  pub(crate) realip_remote_addr: String,
  pub(crate) server_protocol: String,
  pub(crate) request_method: String,
  pub(crate) content_length: i64,
  pub(crate) status: i32,
  pub(crate) request_time: f64,
  pub(crate) body_bytes_sent: i64,
  pub(crate) proxy_host: Option<String>,
  pub(crate) upstream_addr: Option<String>,
  pub(crate) query_string: Option<String>,
  pub(crate) request_body: Option<String>,
  pub(crate) content_type: Option<String>,
  pub(crate) http_user_agent: Option<String>,
  pub(crate) http_referrer: Option<String>,
  pub(crate) http_accept_language: Option<String>,
}

impl Nanocld {
  pub async fn watch_nginx_logs(&self) -> Result<(), NanocldError> {
    let mut res = self.get(String::from("/nginx/logs")).send().await?;
    let status = res.status();

    println!("{:#?}", res);
    println!("{:#?}", status);

    is_api_error(&mut res, &status).await?;

    println!("into stream");
    let mut stream = res.into_stream();

    while let Some(res) = stream.next().await {
      match res {
        Err(err) => {
          eprintln!("{}", err);
          break;
        }
        Ok(data) => {
          let result = &String::from_utf8(data.to_vec()).unwrap();
          let json: NginxLogItem = serde_json::from_str(result).unwrap();
          println!("==== [LOG] ====");
          println!("{}", &serde_json::to_string_pretty(&json).unwrap());
          println!("====       ====");
        }
      }
    }

    Ok(())
  }
}
