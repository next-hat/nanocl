use nanocl_utils::io_error::IoResult;

use crate::utils;
use crate::config::CommandConfig;
use crate::models::{Context, ContextArgs, ContextCommands, ContextRow};

fn exec_list(context: &Context) -> IoResult<()> {
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

fn exec_use(name: &str) -> IoResult<()> {
  Context::r#use(name)?;
  Ok(())
}

fn exec_from(path: &str) -> IoResult<()> {
  let context = Context::read(path)?;
  Context::write(&context)?;
  Ok(())
}

pub async fn exec_context(
  cmd_conf: &CommandConfig<&ContextArgs>,
) -> IoResult<()> {
  let args = cmd_conf.args;
  let context = &cmd_conf.context;
  match &args.commands {
    ContextCommands::List => exec_list(context)?,
    ContextCommands::Use { name } => exec_use(name)?,
    ContextCommands::From { path } => exec_from(path)?,
  }
  Ok(())
}
