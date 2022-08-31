use futures::{TryStreamExt, StreamExt};

use crate::models::*;

use super::{
  http_client::Nanocld,
  error::{NanocldError, is_api_error},
};

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
