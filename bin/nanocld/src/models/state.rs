use nanocl_models::state::{StateDeployment, StateCargo, StateResources};

#[derive(Debug)]
pub enum StateData {
  Deployment(StateDeployment),
  Cargo(StateCargo),
  Resource(StateResources),
}
