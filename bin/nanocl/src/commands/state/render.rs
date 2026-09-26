use std::str::FromStr;

use nanocl_error::io::{FromIo, IoResult};

use crate::{config::CliConfig, models::DisplayFormat, utils};

use super::{
  ArgParseMode, gen_client, hook_cargoes, inject_data, inject_namespace,
  parse_build_args, read_state_file,
};

/// Function called when running `nanocl state render`
pub(super) async fn exec_state_render(
  cli_conf: &CliConfig,
  opts: &crate::models::StateRenderOpts,
) -> IoResult<()> {
  let display_format = cli_conf.user_config.display_format.clone();
  let state_ref = read_state_file(&opts.source, &display_format).await?;
  let args =
    parse_build_args(&state_ref.data, ArgParseMode::Apply, &opts.args, false)?;
  let client = gen_client(cli_conf, &state_ref)?;
  let mut namespace = state_ref
    .data
    .namespace
    .clone()
    .unwrap_or_else(|| "global".to_owned());
  namespace = inject_namespace(&namespace, &args)?;
  let mut rendered =
    inject_data(&state_ref, &args, &cli_conf.context, &client).await?;
  rendered.data.namespace = Some(namespace);
  if let Some(cargoes) = rendered.data.cargoes.clone() {
    let hooked_cargoes = hook_cargoes(cargoes)?;
    rendered.data.cargoes = Some(hooked_cargoes);
  }
  match &opts.output {
    None => {
      // If output is not set, just print the rendered content
      let content = utils::state::stringify_state_for_format(
        &rendered.data,
        &display_format,
      )?;
      println!("{content}");
      Ok(())
    }
    Some(output) => {
      let out_path = std::path::Path::new(&output);
      let ext = out_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("yaml")
        .to_lowercase();
      let display_format =
        DisplayFormat::from_str(&ext).unwrap_or(DisplayFormat::Yaml);
      let content = utils::state::stringify_state_for_format(
        &rendered.data,
        &display_format,
      )?;
      if !opts.skip_confirm {
        println!("{content}");
        utils::dialog::confirm("Are you sure to write this rendered state ?")
          .map_err(|err| err.map_err_context(|| "StateRender"))?;
      }
      if let Some(parent) = out_path.parent()
        && !parent.as_os_str().is_empty()
      {
        std::fs::create_dir_all(parent)?;
      }
      std::fs::write(out_path, content)?;
      println!("Rendered statefile written to {}", out_path.display());
      Ok(())
    }
  }
}
