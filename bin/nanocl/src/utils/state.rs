use crate::error::CliError;
use nanocl_models::state::StateConfig;

pub fn get_file_meta(data: &str) -> Result<StateConfig, CliError> {
  let meta = serde_yaml::from_str::<StateConfig>(data)?;

  Ok(meta)
}
