use std::collections::HashMap;

use regex::Regex;
use liquid::ObjectView;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use nanocld_client::stubs::state::{StateStream, StateStreamStatus};

use nanocl_error::io::{IoError, IoResult, FromIo};

use crate::models::{DisplayFormat, StateRef};

/// Extract metadata eg: `ApiVersion`, `Kind` from a Statefile
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
      Ok(StateRef {
        raw: raw.to_owned(),
        format: DisplayFormat::Yaml,
        data,
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

/// Compile a Statefile with given data
pub fn compile(raw: &str, obj: &dyn ObjectView) -> IoResult<String> {
  // replace "${{ }}" with "{{ }}" syntax for liquid
  let reg = Regex::new(r"\$\{\{(.+?)\}\}")
    .map_err(|err| IoError::invalid_data("Regex", &format!("{err}")))?;
  let template = reg.replace_all(raw, "{{ $1 }}");
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

/// Update the progress bar for a given state stream
pub fn update_progress(
  multiprogress: &MultiProgress,
  layers: &mut HashMap<String, ProgressBar>,
  id: &str,
  state_stream: &StateStream,
) {
  if let Some(pg) = layers.get(id) {
    pg.set_prefix(format!(
      "{:#?}:{}",
      &state_stream.status, &state_stream.kind
    ));
    if let Some(ctx) = &state_stream.context {
      pg.set_message(format!("{}: {}", state_stream.key, ctx));
    }
    if state_stream.status != StateStreamStatus::Pending {
      pg.finish();
    }
  } else {
    let spinner_style =
      ProgressStyle::with_template("{spinner} {prefix:.bold} {wide_msg}")
        .unwrap()
        .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈-");
    let pg = multiprogress.add(ProgressBar::new(1));
    pg.enable_steady_tick(std::time::Duration::from_millis(50));
    pg.set_style(spinner_style);
    pg.set_message(state_stream.key.to_owned());
    pg.set_prefix(format!(
      "{:#?}:{}",
      &state_stream.status, &state_stream.kind
    ));
    layers.insert(id.to_owned(), pg);
  }
}
