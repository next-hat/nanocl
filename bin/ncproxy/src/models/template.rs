use serde::{Serialize, Deserialize};

use nanocl_error::io::{IoResult, IoError};

#[derive(Debug, Serialize, Deserialize)]
pub struct LocationTemplate {
  pub path: String,
  pub upstream_key: String,
  pub redirect: Option<String>,
  pub allowed_ips: Option<Vec<String>>,
  pub version: Option<f64>,
  pub headers: Option<Vec<String>>,
}

pub struct Template<'a> {
  pub data: &'a str,
}

impl<'a> Template<'a> {
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

pub const UPSTREAM_TEMPLATE: &Template = &Template {
  data: include_str!("templates/upstream.conf"),
};

pub const UNIX_UPSTREAM_TEMPLATE: &Template = &Template {
  data: include_str!("templates/unix_upstream.conf"),
};
