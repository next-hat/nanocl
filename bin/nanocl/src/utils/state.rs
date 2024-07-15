use liquid::ObjectView;
use regex::Regex;

use crate::models::{DisplayFormat, StateRef, StateRoot};
use nanocl_error::io::{FromIo, IoError, IoResult};

use super::liquid::StateSource;

pub fn get_format<Path: AsRef<std::path::Path>>(
  format: &DisplayFormat,
  path: Path,
) -> String {
  let default_format = format.to_string();
  let ext = path
    .as_ref()
    .extension()
    .unwrap_or(std::ffi::OsStr::new(&default_format))
    .to_str();
  ext.unwrap_or_default().to_owned()
}

pub async fn download_statefile(url: &str) -> IoResult<(String, String)> {
  let url = if url.starts_with("http://") || url.starts_with("https://") {
    url.to_owned()
  } else {
    format!("http://{url}")
  };
  let client = ntex::http::Client::default();
  let mut res = client.get(&url).send().await.map_err(|err| {
    err.map_err_context(|| "Unable to get Statefile from url")
  })?;
  if res.status().is_redirection() {
    let location = res
      .headers()
      .get("location")
      .ok_or_else(|| IoError::invalid_data("Location", "is not specified"))?
      .to_str()
      .map_err(|err| IoError::invalid_data("Location", &format!("{err}")))?;
    res = client.get(location).send().await.map_err(|err| {
      err.map_err_context(|| "Unable to get Statefile from url")
    })?;
  }
  let data = res
    .body()
    .await
    .map_err(|err| err.map_err_context(|| "Cannot read response from url"))?
    .to_vec();
  let data = std::str::from_utf8(&data).map_err(|err| {
    IoError::invalid_data("From utf8".into(), format!("{err}"))
  })?;
  Ok((url, data.to_owned()))
}

/// Extract metadata eg: `ApiVersion`, `Kind` from a Statefile
/// and return a StateRef with the raw data and the format
pub fn get_state_ref<T>(
  ext: &str,
  path: &str,
  raw: &str,
  root: StateRoot,
) -> IoResult<StateRef<T>>
where
  T: serde::Serialize + serde::de::DeserializeOwned,
{
  match ext {
    "yaml" | "yml" => {
      let data: T = serde_yaml::from_str(raw).map_err(|err| {
        err.map_err_context(|| "Unable to parse Statefile in yaml format")
      })?;
      Ok(StateRef {
        raw: raw.to_owned(),
        format: DisplayFormat::Yaml,
        data,
        location: path.to_owned(),
        root,
      })
    }
    "json" => {
      let data: T = serde_json::from_str(raw).map_err(|err| {
        err.map_err_context(|| "Unable to parse Statefile in json format")
      })?;
      Ok(StateRef {
        raw: raw.to_owned(),
        format: DisplayFormat::Json,
        data,
        location: path.to_owned(),
        root,
      })
    }
    "toml" => {
      let data: T = toml::from_str(raw).map_err(|err| {
        IoError::invalid_data(
          "Unable to parse Statefile in toml format",
          &err.to_string(),
        )
      })?;
      Ok(StateRef {
        raw: raw.to_owned(),
        format: DisplayFormat::Toml,
        data,
        location: path.to_owned(),
        root,
      })
    }
    _ => Err(IoError::invalid_data(
      "State file",
      &format!("Unsupported file extension: {}", ext),
    )),
  }
}

/// Serialize a Statefile for given format eg: yaml, json, toml and given data
pub fn serialize_ext<T>(format: &DisplayFormat, data: &str) -> IoResult<T>
where
  T: serde::Serialize + serde::de::DeserializeOwned,
{
  match format {
    DisplayFormat::Yaml => {
      Ok(serde_yaml::from_str::<T>(data).map_err(|err| {
        err.map_err_context(|| "Unable to deserialize state file")
      })?)
    }
    DisplayFormat::Json => {
      Ok(serde_json::from_str::<T>(data).map_err(|err| {
        err.map_err_context(|| "Unable to deserialize state file")
      })?)
    }
    DisplayFormat::Toml => Ok(toml::from_str::<T>(data).map_err(|err| {
      IoError::invalid_data(
        "Unable to deserialize state file",
        &err.to_string(),
      )
    })?),
  }
}

/// Compile a template with given object using liquid syntax
pub fn compile(
  raw: &str,
  obj: &dyn ObjectView,
  root: StateRoot,
) -> IoResult<String> {
  // replace "${{ }}" with "{{ }}" syntax for liquid
  let reg = Regex::new(r"\$\{\{(.+?)\}\}")
    .map_err(|err| IoError::invalid_data("Regex", &format!("{err}")))?;
  let template_file = reg.replace_all(raw, "{{ $1 }}");
  let template = liquid::ParserBuilder::with_stdlib()
    .partials(liquid::partials::LazyCompiler::new(StateSource { root }))
    .build()
    .unwrap()
    .parse(&template_file)
    .map_err(|err| {
      IoError::invalid_data("Template parsing", &format!("{err}"))
    })?;
  let output = template.render(&obj).map_err(|err| {
    IoError::invalid_data("Template rendering", &format!("{err}"))
  })?;
  Ok(output)
}
