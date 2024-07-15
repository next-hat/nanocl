use nanocl_error::io::IoResult;

use crate::config::CliConfig;
use crate::models::{Context, ContextArg, ContextCommand, ContextRow};
use crate::utils;

/// Function that execute when running `nanocl context ls`
/// Will print the list of contexts
fn exec_context_list(context: &Context) -> IoResult<()> {
  let list = Context::list()?;
  let list = list
    .iter()
    .map(|row| {
      if row.name == context.name {
        return ContextRow {
          name: format!("{} *", row.name),
          description: row.description.clone(),
          endpoint: row.endpoint.clone(),
          current: "✓".into(),
        };
      }
      row.clone()
    })
    .collect::<Vec<ContextRow>>();
  utils::print::print_table(list);
  Ok(())
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
pub async fn exec_context(
  cli_conf: &CliConfig,
  args: &ContextArg,
) -> IoResult<()> {
  let context = &cli_conf.context;
  match &args.command {
    ContextCommand::List => exec_context_list(context)?,
    ContextCommand::Use { name } => exec_context_use(name)?,
    ContextCommand::From { path } => exec_context_from(path)?,
  }
  Ok(())
}
