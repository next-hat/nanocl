use std::{sync::Arc, collections::HashMap};

use ntex::rt;
use futures_util::lock::Mutex;

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
