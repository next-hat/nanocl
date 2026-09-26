use nanocl_error::io::{IoError, IoResult};
use nanocld_client::stubs::statefile::Statefile;

use crate::{models::DisplayFormat, utils};

/// Display statefile documentation in terminal
pub(super) async fn execute_man(source: &str) -> IoResult<()> {
  let (location, raw) =
    if let Ok(path) = std::path::Path::new(source).canonicalize() {
      let data = std::fs::read_to_string(&path)?;
      let loc = path.to_string_lossy().to_string();
      (loc, data)
    } else {
      let (url, data) = utils::state::download_statefile(source).await?;
      (url, data)
    };
  let ext = std::path::Path::new(&location)
    .extension()
    .and_then(|e| e.to_str())
    .unwrap_or("yaml");
  let (state, ext) = match ext {
    "yaml" | "yml" => (
      serde_yaml::from_str::<Statefile>(&raw)
        .map_err(|err| IoError::invalid_data("YAML", &err.to_string()))?,
      DisplayFormat::Yaml,
    ),
    "json" => (
      serde_json::from_str::<Statefile>(&raw)
        .map_err(|err| IoError::invalid_data("JSON", &err.to_string()))?,
      DisplayFormat::Json,
    ),
    "toml" => (
      toml::from_str::<Statefile>(&raw)
        .map_err(|err| IoError::invalid_data("TOML", &err.to_string()))?,
      DisplayFormat::Toml,
    ),
    _ => (
      serde_yaml::from_str::<Statefile>(&raw)
        .map_err(|err| IoError::invalid_data("YAML", &err.to_string()))?,
      DisplayFormat::Yaml,
    ),
  };
  let mut striped_state = state.clone();
  striped_state.metadata = None;
  striped_state.args = None;
  let raw = match ext {
    DisplayFormat::Yaml => serde_yaml::to_string(&striped_state)
      .map_err(|err| IoError::invalid_data("YAML", &err.to_string()))?,
    DisplayFormat::Json => serde_json::to_string_pretty(&striped_state)
      .map_err(|err| IoError::invalid_data("JSON", &err.to_string()))?,
    DisplayFormat::Toml => toml::to_string_pretty(&striped_state)
      .map_err(|err| IoError::invalid_data("TOML", &err.to_string()))?,
  };
  let metadata = state.metadata.unwrap_or_default();
  let mut name = metadata.name.unwrap_or_else(|| source.to_string());
  if let Some(about) = &metadata.about {
    name.push_str(&format!(" - {about}"));
  }
  let mut markdown = String::new();
  markdown.push_str("# Statefile Manual\n\n");
  if let Some(man_content) = &metadata.man_content {
    markdown.push_str(man_content);
    markdown.push_str(&format!("\n## Content\n```{ext}\n{raw}\n```\n"));
    utils::markdown::display(&markdown)?;
    return Ok(());
  }
  markdown.push_str(&format!("## Name\n{name}\n\n"));
  if let Some(tags) = metadata.tags
    && !tags.is_empty()
  {
    let tags = tags.join(", ");
    markdown.push_str(&format!("## Tags\n{tags}\n\n"));
  }
  markdown.push_str(&format!(
    "## Synopsis\nnanocl state apply -s {source} -- [--help] **ARGUMENTS**\nnanocl state rm -s {source} -- [--help] **ARGUMENTS**\n\n"
  ));
  if let Some(long_about) = &metadata.long_about {
    markdown.push_str(&format!("## Description\n{long_about}\n"));
  } else if let Some(about) = &metadata.about {
    markdown.push_str(&format!("## Description\n{about}\n"));
  }
  if let Some(args) = state.args
    && !args.is_empty()
  {
    markdown.push_str("## Arguments\n");
    for a in args.iter() {
      let name = &a.name;
      let kind = &a.kind;
      let required = if a.required { " (required)" } else { "" };
      let multiple = if a.multiple { " (multiple)" } else { "" };
      let description = match &a.description {
        Some(d) => &format!("\n{d}"),
        None => "",
      };
      let default = match &a.default {
        Some(d) => &format!("\nDefault: {d}"),
        None => "",
      };
      markdown.push_str(&format!(
        "**--{name}** {kind}{required}{multiple}{description}{default}\n\n"
      ));
    }
  }
  markdown.push_str(&format!("## Content\n```{ext}\n{raw}\n```\n"));
  utils::markdown::display(&markdown)?;
  Ok(())
}
