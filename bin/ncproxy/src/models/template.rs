use nanocld_client::stubs::proxy::{LimitReq, ProxySslConfig};
use serde::{Deserialize, Serialize};

use nanocl_error::io::{IoError, IoResult};

pub const UPSTREAM_TEMPLATE: &Template = &Template {
  data: include_str!("templates/upstream.conf"),
};

pub const UNIX_UPSTREAM_TEMPLATE: &Template = &Template {
  data: include_str!("templates/unix_upstream.conf"),
};

pub const CONF_TEMPLATE: &Template = &Template {
  data: include_str!("templates/nginx.conf"),
};

pub const STREAM_TEMPLATE: &Template = &Template {
  data: include_str!("templates/stream.conf"),
};

pub const HTTP_TEMPLATE: &Template = &Template {
  data: include_str!("templates/http.conf"),
};

#[derive(Debug, Serialize, Deserialize)]
pub struct LocationTemplate {
  pub path: String,
  pub enable_logging: bool,
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn default_config_disables_access_logs_and_defines_safe_text_formats() {
    let output = CONF_TEMPLATE
      .compile(&liquid::object!({
        "nginx_dir": "/etc/nginx",
        "state_dir": "/tmp/ncproxy",
        "health_generation": "test-generation",
      }))
      .unwrap();

    assert!(output.contains("listen 443 ssl default_server reuseport;"));
    assert!(output.contains("listen [::]:443 ssl default_server reuseport;"));
    assert!(output.contains("ssl_reject_handshake on;"));
    for sensitive in [
      "$query_string",
      "$request_body",
      "$http_referer",
      "$request_uri",
      "$args",
    ] {
      assert!(!output.contains(sensitive), "logs contain {sensitive}");
    }
    assert!(!output.contains("\"$request\""));
    let access_logs = output
      .lines()
      .map(str::trim)
      .filter(|line| line.starts_with("access_log "))
      .collect::<Vec<_>>();
    assert_eq!(access_logs, vec!["access_log off;"; 3]);
    assert_eq!(
      output
        .matches("error_log /tmp/ncproxy/log/error.log warn;")
        .count(),
      2
    );
    assert!(
      output
        .contains("log_format http_basic '$remote_addr - - [$time_local] '")
    );
    assert!(output.contains("\"$request_method $uri $server_protocol\""));
    assert!(output.contains("log_format stream_basic"));
    assert!(!output.contains("escape=json"));
    assert!(!output.contains("http_json"));
    assert!(!output.contains("stream_json"));
  }

  #[test]
  fn default_config_exposes_worker_probe_only_on_private_socket() {
    let output = CONF_TEMPLATE
      .compile(&liquid::object!({
        "nginx_dir": "/etc/nginx",
        "state_dir": "/tmp/ncproxy",
        "health_generation": "test-generation",
      }))
      .unwrap();
    let probe_start = output
      .find("listen unix:/tmp/ncproxy/nginx-health.sock;")
      .unwrap();
    let probe = &output
      [probe_start..output[probe_start..].find("  ##").unwrap() + probe_start];
    assert!(probe.contains("access_log off;"));
    assert!(
      probe.contains("return 200 \"ncproxy-health $pid test-generation\\n\";")
    );
    assert_eq!(probe.matches("listen ").count(), 1);
  }

  #[test]
  fn stream_access_logging_uses_a_separate_text_file_only_when_enabled() {
    for enabled in [false, true] {
      let output = STREAM_TEMPLATE
        .compile(&liquid::object!({
          "key": "logging-test",
          "listen": "127.0.0.1:9000",
          "state_dir": "/tmp/ncproxy",
          "upstream_key": "upstream",
          "enable_logging": enabled,
        }))
        .unwrap();
      let directive = if enabled {
        "access_log /tmp/ncproxy/log/stream.log stream_basic;"
      } else {
        "access_log off;"
      };
      assert!(output.contains(directive));
      assert_eq!(output.matches("access_log ").count(), 1);
      assert!(!output.contains("http_basic"));
      assert!(!output.contains("_json"));
    }
  }

  #[test]
  fn unavailable_tls_rejects_handshakes_without_advertising_http3() {
    let output = HTTP_TEMPLATE
      .compile(&liquid::object!({
        "key": "missing-tls",
        "listen": "0.0.0.0:443",
        "listen_https": "0.0.0.0:443",
        "domain": "h3.test",
        "http3": true,
        "hide_upstream": true,
      }))
      .unwrap();
    assert!(output.contains("listen 0.0.0.0:443 ssl;"));
    assert!(output.contains("ssl_reject_handshake on;"));
    assert!(!output.contains("listen 0.0.0.0:443;"));
    assert!(!output.contains("quic"));
    assert!(!output.contains("http3 on;"));
    assert!(!output.contains("Alt-Svc"));
    assert!(!output.contains("proxy_pass"));
    assert!(!output.contains("ssl_certificate "));
  }

  #[test]
  fn http3_with_tls_renders_quic_on_the_requested_virtual_host() {
    let output = HTTP_TEMPLATE.compile(&liquid::object!({
      "key": "http3",
      "listen_https": "0.0.0.0:8443",
      "domain": "h3.test",
      "http3": true,
      "ssl": {"Certificate": "/tmp/test.cert", "CertificateKey": "/tmp/test.key"},
      "locations": Vec::<LocationTemplate>::new(),
      "hide_upstream": false,
    })).unwrap();
    assert!(output.contains("listen 0.0.0.0:8443 ssl;"));
    assert!(output.contains("listen 0.0.0.0:8443 quic;"));
    assert!(output.contains("http3 on;"));
    assert!(output.contains("server_name h3.test;"));
    assert!(output.contains("ssl_certificate         /tmp/test.cert;"));
    assert!(output.contains("h3=\":8443\""));
    assert!(!output.contains("ssl_reject_handshake"));
  }

  #[test]
  fn renders_location_proxy_options_and_preserves_omissions() {
    let location = LocationTemplate {
      path: "/v2/".into(),
      enable_logging: false,
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
          enable_logging: false,
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
