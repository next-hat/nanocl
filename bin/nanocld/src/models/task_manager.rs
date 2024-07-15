use std::{collections::HashMap, sync::Arc};

use futures_util::lock::Mutex;
use ntex::rt;

use nanocl_error::io::IoResult;

use nanocl_stubs::system::NativeEventAction;

#[derive(Clone)]
pub struct ObjTask {
  pub kind: NativeEventAction,
  pub fut: Arc<rt::JoinHandle<IoResult<()>>>,
}

#[derive(Clone, Default)]
pub struct TaskManager {
  pub tasks: Arc<Mutex<HashMap<String, ObjTask>>>,
}
