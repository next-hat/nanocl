use liquid::ObjectView;
use regex::Regex;

use nanocld_client::stubs::state::StateMeta;

use nanocl_utils::io_error::{IoError, IoResult, FromIo};

use crate::models::{DisplayFormat, StateRef};

/// Extract metadata eg: ApiVersion, Type from a Statefile
/// and return a StateRef with the raw data and the format
pub fn get_state_ref<T>(ext: &str, raw: &str) -> IoResult<StateRef<T>>
where
  T: serde::Serialize + serde::de::DeserializeOwned,
{
  match ext {
    "yaml" | "yml" => {
      let data: T = serde_yaml::from_str(raw).map_err(|err| {
        err.map_err_context(|| "Unable to parse Statefile in yaml format")
      })?;

      let state_ref = serde_yaml::from_str::<StateMeta>(raw)
        .map_err(|err| {
          err.map_err_context(|| {
            "Unable to extract meta from state file in yaml format"
          })
        })
        .map(|meta| StateRef {
          raw: raw.to_owned(),
          format: DisplayFormat::Yaml,
          meta,
          data,
        })?;
      Ok(state_ref)
    }
    "json" => {
      let data: T = serde_json::from_str(raw).map_err(|err| {
        err.map_err_context(|| "Unable to parse Statefile in json format")
      })?;

      let state_ref = serde_json::from_str::<StateMeta>(raw)
        .map_err(|err| {
          err.map_err_context(|| {
            "Unable to extract meta from state file in json format"
          })
        })
        .map(|meta| StateRef {
          raw: raw.to_owned(),
          format: DisplayFormat::Json,
          meta,
          data,
        })?;

      Ok(state_ref)
    }
    "toml" => {
      let data: T = toml::from_str(raw).map_err(|err| {
        IoError::invalid_data(
          "Unable to parse Statefile in toml format",
          &err.to_string(),
        )
      })?;

      let state_ref = toml::from_str::<StateMeta>(raw)
        .map_err(|err| {
          IoError::invalid_data(
            "Unable to extract meta from state file in toml format",
            &err.to_string(),
          )
        })
        .map(|meta| StateRef {
          raw: raw.to_owned(),
          format: DisplayFormat::Toml,
          meta,
          data,
        })?;

      Ok(state_ref)
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

/// Compile a StateFile with given data
pub fn compile(raw: &str, obj: &dyn ObjectView) -> IoResult<String> {
  // replace "${{ }}" with "{{ }}" syntax for liquid
  let reg = Regex::new(r"\$\{\{(.+?)\}\}")
    .map_err(|err| IoError::invalid_data("Regex", &format!("{err}")))?;
  let template = reg.replace_all(raw, "{{ $1 }}").to_string();

  let template = liquid::ParserBuilder::with_stdlib()
    .build()
    .unwrap()
    .parse(&template)
    .map_err(|err| {
      IoError::invalid_data("Template parsing", &format!("{err}"))
    })?;

  let output = template.render(&obj).map_err(|err| {
    IoError::invalid_data("Template rendering", &format!("{err}"))
  })?;
  Ok(output)
}
