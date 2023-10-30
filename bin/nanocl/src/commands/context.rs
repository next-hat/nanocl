use nanocl_error::io::IoResult;

use crate::utils;
use crate::config::CliConfig;
use crate::models::{Context, ContextArg, ContextCommand, ContextRow};

/// ## Exec context list
///
/// Function that execute when running `nanocl context ls`
/// Will print the list of contexts
///
/// ## Arguments
///
/// * [context](Context) The context
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_error::io::IoError) An error occured
///
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

/// ## Exec context use
///
/// Function that execute when running `nanocl context use`
/// Will use the selected context as the current context
///
/// ## Arguments
///
/// * [name](str) The name of the context to use
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_error::io::IoError) An error occured
///
fn exec_context_use(name: &str) -> IoResult<()> {
  Context::r#use(name)?;
  Ok(())
}

/// ## Exec context from
///
/// Function that execute when running `nanocl context from`
/// Will import a context from a file
///
/// ## Arguments
///
/// * [path](str) The path to the file
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_error::io::IoError) An error occured
///
fn exec_context_from(path: &str) -> IoResult<()> {
  let context = Context::read(path)?;
  Context::write(&context)?;
  Ok(())
}

/// ## Exec context
///
/// Function that execute when running `nanocl context`
///
/// ## Arguments
///
/// * [cli_conf](CliConfig) The cli config
/// * [args](ContextArg) The context options
///
/// ## Return
///
/// * [Result](Result) The result of the operation
///   * [Ok](()) The operation was successful
///   * [Err](nanocl_error::io::IoError) An error occured
///
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
