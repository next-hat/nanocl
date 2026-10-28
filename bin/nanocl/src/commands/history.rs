use nanocl_error::io::{IoError, IoResult};
use serde::Serialize;
use tabled::Tabled;
use uuid::Uuid;

use crate::{
  models::{DisplayFormat, GenericHistoryOpts},
  utils,
};

/// Reject unsupported formats and invalid pages before requesting history.
pub(super) fn validate_options(opts: &GenericHistoryOpts) -> IoResult<()> {
  if !(1..=100).contains(&opts.limit.unwrap_or(50)) {
    return Err(IoError::invalid_input(
      "History",
      "History limit must be between 1 and 100",
    ));
  }
  if opts.offset.unwrap_or(0) < 0 {
    return Err(IoError::invalid_input(
      "History",
      "History offset must not be negative",
    ));
  }
  if opts.id.is_some() && (opts.limit.is_some() || opts.offset.is_some()) {
    return Err(IoError::invalid_input(
      "History",
      "--id cannot be combined with --limit or --offset",
    ));
  }
  if opts.id.is_none()
    && matches!(opts.display.as_ref(), Some(DisplayFormat::Toml))
  {
    return Err(IoError::invalid_input(
      "History",
      "TOML output requires --id ID; use yaml or json for a history list",
    ));
  }
  Ok(())
}

pub(super) fn print_list<T: Serialize, R: Tabled>(
  opts: &GenericHistoryOpts,
  items: Vec<T>,
  row: impl FnMut(T) -> R,
) -> IoResult<()> {
  print!("{}", render_list(opts, items, row)?);
  Ok(())
}

pub(super) fn print_detail<T: Serialize>(
  opts: &GenericHistoryOpts,
  default_format: &DisplayFormat,
  item: T,
) -> IoResult<()> {
  print!("{}", render_detail(opts, default_format, item)?);
  Ok(())
}

/// Select a revision from the complete history, then paginate list output.
pub(super) fn print_revision_history<T: Serialize, R: Tabled>(
  opts: &GenericHistoryOpts,
  default_format: &DisplayFormat,
  items: Vec<T>,
  id: impl Fn(&T) -> Uuid,
  row: impl FnMut(T) -> R,
) -> IoResult<()> {
  print!(
    "{}",
    render_revision_history(opts, default_format, items, id, row)?
  );
  Ok(())
}

fn render_format<T: Serialize>(
  format: &DisplayFormat,
  item: T,
) -> IoResult<String> {
  match format {
    DisplayFormat::Json => {
      serde_json::to_string_pretty(&item).map_err(|error| {
        IoError::invalid_data("History output", &error.to_string())
      })
    }
    DisplayFormat::Yaml => serde_yaml::to_string(&item).map_err(|error| {
      IoError::invalid_data("History output", &error.to_string())
    }),
    DisplayFormat::Toml => toml::to_string(&item).map_err(|error| {
      IoError::invalid_data(
        "History output",
        &format!(
          "TOML cannot represent this history data; use --display json or --display yaml: {error}"
        ),
      )
    }),
  }
}

fn render_list<T: Serialize, R: Tabled>(
  opts: &GenericHistoryOpts,
  items: Vec<T>,
  row: impl FnMut(T) -> R,
) -> IoResult<String> {
  validate_options(opts)?;
  if let Some(format) = &opts.display {
    return render_format(format, items);
  }
  let mut table = utils::print::render_table(items.into_iter().map(row));
  if !table.is_empty() {
    table.push('\n');
  }
  Ok(table)
}

fn render_detail<T: Serialize>(
  opts: &GenericHistoryOpts,
  default_format: &DisplayFormat,
  item: T,
) -> IoResult<String> {
  validate_options(opts)?;
  render_format(opts.display.as_ref().unwrap_or(default_format), item)
}

fn render_revision_history<T: Serialize, R: Tabled>(
  opts: &GenericHistoryOpts,
  default_format: &DisplayFormat,
  items: Vec<T>,
  id: impl Fn(&T) -> Uuid,
  row: impl FnMut(T) -> R,
) -> IoResult<String> {
  validate_options(opts)?;
  if let Some(selected_id) = opts.id {
    let item = items
      .into_iter()
      .find(|item| id(item) == selected_id)
      .ok_or_else(|| {
        IoError::not_found(
          "History".to_owned(),
          format!("History entry {selected_id} not found for {}", opts.key),
        )
      })?;
    return render_detail(opts, default_format, item);
  }
  let offset = usize::try_from(opts.offset.unwrap_or(0)).unwrap_or(usize::MAX);
  let limit = opts.limit.unwrap_or(50) as usize;
  let page = items.into_iter().skip(offset).take(limit).collect();
  render_list(opts, page, row)
}

#[cfg(test)]
mod tests {
  use nanocld_client::stubs::job::JobSpec;

  use super::*;
  use crate::models::SpecHistoryRow;

  #[derive(Clone, Serialize)]
  #[serde(rename_all = "PascalCase")]
  struct Revision {
    key: Uuid,
    version: String,
  }

  fn revision(index: u128) -> Revision {
    Revision {
      key: Uuid::from_u128(index),
      version: format!("revision-{index}"),
    }
  }

  fn options() -> GenericHistoryOpts {
    GenericHistoryOpts {
      key: "global.backup".into(),
      id: None,
      limit: None,
      offset: None,
      display: None,
    }
  }

  fn row(item: Revision) -> SpecHistoryRow {
    SpecHistoryRow {
      id: item.key.to_string(),
      version: item.version,
      created_at: "2026-10-27 12:00:00".into(),
    }
  }

  fn render_revisions(opts: &GenericHistoryOpts) -> IoResult<String> {
    render_revision_history(
      opts,
      &DisplayFormat::Yaml,
      (1..=60).map(revision).collect(),
      |item| item.key,
      row,
    )
  }

  #[test]
  fn history_list_renders_table_or_requested_json_yaml() {
    let mut opts = options();
    let item = revision(1);
    let table = render_list(&opts, vec![item.clone()], row).unwrap();
    for expected in [
      "ID",
      "VERSION",
      "CREATED AT",
      "revision-1",
      &item.key.to_string(),
    ] {
      assert!(table.contains(expected), "{table}");
    }
    opts.display = Some(DisplayFormat::Json);
    let json = render_list(&opts, vec![item.clone()], row).unwrap();
    assert_eq!(
      serde_json::from_str::<serde_json::Value>(&json).unwrap(),
      serde_json::to_value(vec![item.clone()]).unwrap()
    );
    opts.display = Some(DisplayFormat::Yaml);
    let yaml = render_list(&opts, vec![item.clone()], row).unwrap();
    assert_eq!(
      serde_yaml::from_str::<serde_json::Value>(&yaml).unwrap(),
      serde_json::to_value(vec![item]).unwrap()
    );
  }

  #[test]
  fn history_detail_uses_configured_format_and_explicit_toml() {
    let mut opts = options();
    opts.id = Some(Uuid::from_u128(1));
    let item = revision(1);
    let yaml =
      render_detail(&opts, &DisplayFormat::Yaml, item.clone()).unwrap();
    assert_eq!(
      serde_yaml::from_str::<serde_json::Value>(&yaml).unwrap(),
      serde_json::to_value(item.clone()).unwrap()
    );
    let json =
      render_detail(&opts, &DisplayFormat::Json, item.clone()).unwrap();
    assert_eq!(
      serde_json::from_str::<serde_json::Value>(&json).unwrap(),
      serde_json::to_value(item.clone()).unwrap()
    );
    opts.display = Some(DisplayFormat::Toml);
    let toml =
      render_detail(&opts, &DisplayFormat::Yaml, item.clone()).unwrap();
    let value: toml::Value = toml::from_str(&toml).unwrap();
    assert_eq!(value["Key"].as_str(), Some(item.key.to_string().as_str()));
    assert_eq!(value["Version"].as_str(), Some("revision-1"));
  }

  #[test]
  fn job_history_detail_guides_toml_null_metadata_to_lossless_formats() {
    let mut opts = options();
    opts.id = Some(Uuid::from_u128(1));
    let mut item = JobSpec {
      key: opts.id.unwrap(),
      job_key: "backup".into(),
      version: "v0.19".into(),
      created_at: chrono::DateTime::UNIX_EPOCH.naive_utc(),
      name: "backup".into(),
      metadata: Some(serde_json::json!({"nested": {"value": null}})),
      ..Default::default()
    };
    let serializer_error = toml::to_string(&item).unwrap_err().to_string();
    for explicit in [false, true] {
      opts.display = explicit.then_some(DisplayFormat::Toml);
      let default_format = if explicit {
        DisplayFormat::Yaml
      } else {
        DisplayFormat::Toml
      };
      let error = render_detail(&opts, &default_format, &item)
        .unwrap_err()
        .to_string();
      for expected in [
        "TOML cannot represent this history data",
        "--display json",
        "--display yaml",
        &serializer_error,
      ] {
        assert!(error.contains(expected), "{error}");
      }
    }
    opts.display = Some(DisplayFormat::Json);
    let json = render_detail(&opts, &DisplayFormat::Toml, &item).unwrap();
    assert_eq!(serde_json::from_str::<JobSpec>(&json).unwrap(), item);
    opts.display = Some(DisplayFormat::Yaml);
    let yaml = render_detail(&opts, &DisplayFormat::Toml, &item).unwrap();
    assert_eq!(serde_yaml::from_str::<JobSpec>(&yaml).unwrap(), item);

    item.metadata = Some(serde_json::json!({"nested": {"value": "present"}}));
    opts.display = None;
    let toml = render_detail(&opts, &DisplayFormat::Toml, &item).unwrap();
    assert_eq!(toml::from_str::<JobSpec>(&toml).unwrap(), item);
  }

  #[test]
  fn history_options_reject_invalid_pages_and_list_toml() {
    let mut opts = options();
    opts.display = Some(DisplayFormat::Toml);
    assert!(
      validate_options(&opts)
        .unwrap_err()
        .to_string()
        .contains("requires --id")
    );
    opts.display = None;
    for limit in [0, 101] {
      opts.limit = Some(limit);
      assert!(validate_options(&opts).is_err());
    }
    opts.limit = None;
    opts.offset = Some(-1);
    assert!(validate_options(&opts).is_err());
    opts.offset = Some(0);
    opts.id = Some(Uuid::from_u128(1));
    assert!(validate_options(&opts).is_err());
    opts.offset = None;
    opts.limit = Some(1);
    assert!(validate_options(&opts).is_err());
    opts.limit = None;
    opts.display = Some(DisplayFormat::Toml);
    assert!(validate_options(&opts).is_ok());
  }

  #[test]
  fn history_revision_pages_use_default_limit_and_requested_offset() {
    let mut opts = options();
    opts.display = Some(DisplayFormat::Json);
    let value: serde_json::Value =
      serde_json::from_str(&render_revisions(&opts).unwrap()).unwrap();
    assert_eq!(value.as_array().unwrap().len(), 50);
    assert_eq!(value[49]["Version"], "revision-50");
    opts.limit = Some(3);
    opts.offset = Some(55);
    let value: serde_json::Value =
      serde_json::from_str(&render_revisions(&opts).unwrap()).unwrap();
    assert_eq!(value.as_array().unwrap().len(), 3);
    assert_eq!(value[0]["Version"], "revision-56");
    assert_eq!(value[2]["Version"], "revision-58");
    opts.offset = Some(i64::MAX);
    let value: serde_json::Value =
      serde_json::from_str(&render_revisions(&opts).unwrap()).unwrap();
    assert_eq!(value, serde_json::json!([]));
  }

  #[test]
  fn history_revision_id_searches_all_rows_and_reports_missing_id() {
    let mut opts = options();
    opts.id = Some(Uuid::from_u128(60));
    let value: serde_json::Value =
      serde_yaml::from_str(&render_revisions(&opts).unwrap()).unwrap();
    assert_eq!(value["Version"], "revision-60");
    let missing = Uuid::from_u128(61);
    opts.id = Some(missing);
    let error = render_revisions(&opts).unwrap_err().to_string();
    assert!(error.contains(&missing.to_string()), "{error}");
    assert!(error.contains(&opts.key), "{error}");
  }
}
