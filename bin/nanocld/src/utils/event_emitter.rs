use nanocl_stubs::system::{NativeEventAction, EventActor, EventPartial, EventKind};

use crate::models::SystemState;

pub fn emit_normal_native_action<A>(
  actor: &A,
  action: NativeEventAction,
  state: &SystemState,
) where
  A: Into<EventActor> + Clone,
{
  let actor = actor.clone().into();
  let event = EventPartial {
    reporting_controller: "nanocl.io/core".to_owned(),
    reporting_node: state.config.hostname.clone(),
    kind: EventKind::Normal,
    action: action.to_string(),
    related: None,
    reason: "state_sync".to_owned(),
    note: None,
    metadata: None,
    actor: Some(actor),
  };
  state.spawn_emit_event(event);
}
