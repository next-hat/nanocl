use nanocld_client::stubs::proxy::{LimitReq, ProxySslConfig};
use serde::{Deserialize, Serialize};

use nanocl_error::io::{IoError, IoResult};

#[derive(Debug, Serialize, Deserialize)]
pub struct LocationTemplate {
  pub path: String,
  pub upstream_key: String,
  pub upstream_path: String,
  pub redirect: Option<String>,
  pub limit_req: Option<LimitReq>,
  pub allowed_ips: Option<Vec<String>>,
  pub version: Option<f64>,
  pub headers: Option<Vec<String>>,
  pub ssl: Option<ProxySslConfig>,
  pub client_max_body_size: Option<String>,
  pub client_body_timeout: Option<String>,
  pub request_buffering: Option<String>,
  pub response_buffering: Option<String>,
  pub cache: Option<String>,
  pub read_timeout: Option<String>,
  pub send_timeout: Option<String>,
  pub connect_timeout: Option<String>,
  pub next_upstream: Option<String>,
  pub intercept_errors: Option<String>,
}

pub struct Template<'a> {
  pub data: &'a str,
}

impl Template<'_> {
  /// Compile a template with given object using liquid syntax
  pub fn compile(&self, obj: &dyn liquid::ObjectView) -> IoResult<String> {
    let template = liquid::ParserBuilder::with_stdlib()
      .build()
      .map_err(|err| {
        IoError::invalid_data("Template parsing", err.to_string().as_str())
      })?
      .parse(self.data)
      .map_err(|err| {
        IoError::invalid_data("Template parsing", err.to_string().as_str())
      })?;
    let output = template.render(&obj).map_err(|err| {
      IoError::invalid_data("Template rendering", err.to_string().as_str())
    })?;
    Ok(output)
  }
}

pub const CONF_TEMPLATE: &Template = &Template {
  data: include_str!("templates/nginx.conf"),
};

pub const STREAM_TEMPLATE: &Template = &Template {
  data: include_str!("templates/stream.conf"),
};

pub const HTTP_TEMPLATE: &Template = &Template {
  data: include_str!("templates/http.conf"),
};

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn renders_location_proxy_options_and_preserves_omissions() {
    let location = LocationTemplate {
      path: "/v2/".into(),
      upstream_key: "http://registry".into(),
      upstream_path: "/".into(),
      redirect: None,
      limit_req: None,
      allowed_ips: None,
      version: None,
      headers: None,
      ssl: None,
      client_max_body_size: Some("0".into()),
      client_body_timeout: Some("900s".into()),
      request_buffering: Some("off".into()),
      response_buffering: Some("off".into()),
      cache: Some("off".into()),
      read_timeout: Some("900s".into()),
      send_timeout: Some("900s".into()),
      connect_timeout: Some("5s".into()),
      next_upstream: Some("off".into()),
      intercept_errors: Some("off".into()),
    };
    let output = HTTP_TEMPLATE
      .compile(&liquid::object!({
        "key": "registry",
        "listen": "127.0.0.1:80",
        "listen_https": "127.0.0.1:443",
        "locations": vec![location],
        "hide_upstream": false,
      }))
      .unwrap();
    let location_start = output.find("location /v2/").unwrap();
    let location_end =
      output[location_start..].find("}").unwrap() + location_start;
    let block = &output[location_start..=location_end];

    for directive in [
      "client_max_body_size 0;",
      "client_body_timeout 900s;",
      "proxy_request_buffering off;",
      "proxy_buffering off;",
      "proxy_cache off;",
      "proxy_read_timeout 900s;",
      "proxy_send_timeout 900s;",
      "proxy_connect_timeout 5s;",
      "proxy_next_upstream off;",
      "proxy_intercept_errors off;",
    ] {
      assert!(block.contains(directive), "missing {directive} in {block}");
    }
  }

  #[test]
  fn omitted_location_options_emit_no_new_directives() {
    let output = HTTP_TEMPLATE
      .compile(&liquid::object!({
        "key": "existing",
        "listen": "127.0.0.1:80",
        "listen_https": "127.0.0.1:443",
        "locations": vec![LocationTemplate {
          path: "/".into(),
          upstream_key: "http://upstream".into(),
          upstream_path: "/".into(),
          redirect: None,
          limit_req: None,
          allowed_ips: None,
          version: None,
          headers: None,
          ssl: None,
          client_max_body_size: None,
          client_body_timeout: None,
          request_buffering: None,
          response_buffering: None,
          cache: None,
          read_timeout: None,
          send_timeout: None,
          connect_timeout: None,
          next_upstream: None,
          intercept_errors: None,
        }],
        "hide_upstream": false,
      }))
      .unwrap();
    assert!(!output.contains("client_body_timeout"));
    assert!(!output.contains("proxy_request_buffering"));
    assert!(
      output.contains("proxy_next_upstream                     error timeout;")
    );
  }
}

pub const UPSTREAM_TEMPLATE: &Template = &Template {
  data: include_str!("templates/upstream.conf"),
};

pub const UNIX_UPSTREAM_TEMPLATE: &Template = &Template {
  data: include_str!("templates/unix_upstream.conf"),
};
