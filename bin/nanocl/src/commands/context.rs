use nanocl_error::io::IoResult;

use crate::config::UserConfig;
use crate::models::{
  Context, ContextArg, ContextCommand, ContextCompactRow, ContextListOpts,
  ContextRow,
};
use crate::utils;

/// Function that execute when running `nanocl context ls`
/// Will print the list of contexts
fn exec_context_list(
  current_name: Option<&str>,
  opts: &ContextListOpts,
) -> IoResult<()> {
  println!(
    "{}",
    render_context_list(Context::list()?, current_name, opts)
  );
  Ok(())
}

fn render_context_list(
  list: Vec<ContextRow>,
  current_name: Option<&str>,
  opts: &ContextListOpts,
) -> String {
  let list = list
    .into_iter()
    .map(|mut row| {
      if current_name == Some(row.name.as_str()) {
        row.name = format!("{} *", row.name);
        row.current = "✓".into();
      } else if current_name.is_none() {
        row.current = "?".into();
      }
      row
    })
    .collect::<Vec<ContextRow>>();
  if opts.wide {
    utils::print::render_table(list)
  } else {
    utils::print::render_table(list.into_iter().map(ContextCompactRow::from))
  }
}

/// Function that execute when running `nanocl context use`
/// Will use the selected context as the current context
fn exec_context_use(name: &str) -> IoResult<()> {
  Context::r#use(name)?;
  Ok(())
}

/// Function that execute when running `nanocl context from`
/// Will import a context from a file
fn exec_context_from(path: &str) -> IoResult<()> {
  let context = Context::read(path)?;
  Context::write(&context)?;
  Ok(())
}

/// Function that execute when running `nanocl context`
pub fn exec_context(args: &ContextArg) -> IoResult<()> {
  Context::ensure()?;
  match &args.command {
    ContextCommand::List(opts) => {
      let current = match UserConfig::new() {
        Ok(config) => Some(config.current_context),
        Err(error) => {
          eprintln!("Warning: current context is unknown: {error}");
          None
        }
      };
      exec_context_list(current.as_deref(), opts)?;
    }
    ContextCommand::Use { name } => exec_context_use(name)?,
    ContextCommand::From { path } => exec_context_from(path)?,
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn context_list_keeps_selection_and_endpoint_with_optional_description() {
    for wide in [false, true] {
      let rows = vec![ContextRow {
        name: "production".into(),
        endpoint: "https://nanocl.example.com".into(),
        description: "Production cluster".into(),
        current: "⨯".into(),
      }];
      let opts = ContextListOpts { wide };
      let output = render_context_list(rows, Some("production"), &opts);
      assert!(output.contains("production *"));
      assert!(output.contains("https://nanocl.example.com"));
      assert!(output.contains("✓"));
      assert_eq!(output.contains("Production cluster"), wide);
      assert_eq!(output.contains("description"), wide);
      let empty = render_context_list(Vec::new(), Some("production"), &opts);
      assert!(empty.contains("name"));
      assert!(empty.contains("endpoint"));
      assert_eq!(empty.contains("description"), wide);
    }
  }

  #[test]
  fn context_list_marks_unknown_selection_without_selecting_default() {
    for wide in [false, true] {
      let rows = vec![ContextRow {
        name: "default".into(),
        endpoint: "unix:///run/nanocl/nanocl.sock".into(),
        description: "Default context".into(),
        current: "⨯".into(),
      }];
      let output = render_context_list(rows, None, &ContextListOpts { wide });
      assert!(output.contains("default"));
      assert!(output.contains("unix:///run/nanocl/nanocl.sock"));
      assert!(output.contains('?'));
      assert!(!output.contains('*'));
      assert!(!output.contains('✓'));
    }
  }
}
