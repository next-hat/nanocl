use std::{
  fs,
  path::{Path, PathBuf},
};
use regex::Regex;
use liquid::{partials::PartialSource, ObjectView};

use nanocl_error::io::{IoError, IoResult, FromIo};
use crate::models::{StateRef, DisplayFormat};

/// Extract metadata eg: `ApiVersion`, `Kind` from a Statefile
/// and return a StateRef with the raw data and the format
pub fn get_state_ref<T>(
  ext: &str,
  raw: &str,
  include_dir: Option<PathBuf>,
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
        include_dir,
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
        include_dir,
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
        include_dir,
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

#[derive(Default, Debug, Clone)]
struct FileSource {
  include_dir: Option<PathBuf>,
}

impl PartialSource for FileSource {
  fn contains(&self, _name: &str) -> bool {
    true
  }

  fn names(&self) -> Vec<&str> {
    vec![]
  }

  fn try_get<'a>(
    &'a self,
    name: &str,
  ) -> std::option::Option<std::borrow::Cow<'a, str>> {
    let mut path = Path::new(name).join("");
    eprintln!("Path: {:?}", path);
    eprintln!("Include dir: {:?}", self.include_dir);
    if let Some(ref dir) = self.include_dir {
      eprintln!("Pathroot: {:?}", path.has_root());
      if !path.has_root() {
        let new_path = Path::new(dir).join(path.clone()).canonicalize();
        eprintln!("New path: {:?}", new_path);
        if let Ok(new_path) = new_path {
          if new_path.exists() && new_path.is_file() {
            path = new_path;
          }
        }
      }
    }

    eprintln!("Path: {:?}", path);
    match path.as_path().canonicalize() {
      Ok(path) => {
        let path = path.to_str().unwrap();
        match fs::read_to_string(path) {
          Ok(content) => Some(content.into()),
          Err(_) => {
            eprintln!("Unable to read partial: {}", name);
            None
          }
        }
      }
      Err(_) => {
        eprintln!("Unable to canonicalize partial: {}", name);
        None
      }
    }
  }
}

/// Compile a template with given object using liquid syntax
pub fn compile(
  raw: &str,
  obj: &dyn ObjectView,
  include_dir: Option<PathBuf>,
) -> IoResult<String> {
  // replace "${{ }}" with "{{ }}" syntax for liquid
  let reg = Regex::new(r"\$\{\{(.+?)\}\}")
    .map_err(|err| IoError::invalid_data("Regex", &format!("{err}")))?;
  let template = reg.replace_all(raw, "{{ $1 }}");

  let template = liquid::ParserBuilder::with_stdlib()
    .partials(liquid::partials::LazyCompiler::new(FileSource {
      include_dir,
    }))
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
