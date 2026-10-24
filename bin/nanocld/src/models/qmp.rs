use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;
use tokio::io::BufReader;
use tokio::net::UnixStream;
use tokio::time::Instant;

/// One bounded QMP session. Only the container utilities expose commands.
pub(crate) struct QmpClient<S = UnixStream> {
  pub(crate) stream: BufReader<S>,
  pub(crate) deadline: Instant,
  pub(crate) next_id: u64,
  /// An interrupted command leaves its outcome uncertain; reconnect to retry.
  pub(crate) failed: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct QmpCpuSlot {
  #[serde(rename = "type")]
  pub(crate) driver: String,
  pub(crate) props: BTreeMap<String, u64>,
  #[serde(rename = "vcpus-count")]
  pub(crate) vcpus_count: u64,
  #[serde(rename = "qom-path")]
  pub(crate) qom_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct QmpMemorySize {
  #[serde(rename = "base-memory")]
  pub(crate) base_memory: u64,
  #[serde(rename = "plugged-memory")]
  pub(crate) plugged_memory: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct QmpMemoryDevice {
  #[serde(rename = "type")]
  pub(crate) kind: String,
  pub(crate) data: QmpMemoryDeviceData,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct QmpMemoryDeviceData {
  pub(crate) id: Option<String>,
  pub(crate) size: u64,
  pub(crate) memdev: String,
  pub(crate) slot: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct QmpBlockDevice {
  pub(crate) device: String,
  pub(crate) qdev: Option<String>,
  pub(crate) removable: bool,
  pub(crate) inserted: Option<QmpBlockNode>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct QmpBlockNode {
  #[serde(rename = "node-name")]
  pub(crate) node_name: String,
  pub(crate) file: String,
  pub(crate) ro: bool,
  pub(crate) drv: String,
  pub(crate) encrypted: bool,
  pub(crate) backing_file_depth: u64,
  pub(crate) backing_file: Option<String>,
  pub(crate) image: QmpBlockImage,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) struct QmpBlockImage {
  pub(crate) filename: String,
  pub(crate) format: String,
  pub(crate) virtual_size: u64,
  pub(crate) encrypted: Option<bool>,
  pub(crate) backing_filename: Option<String>,
  pub(crate) full_backing_filename: Option<String>,
  pub(crate) backing_image: Option<Box<QmpBlockImage>>,
  pub(crate) snapshots: Option<Vec<Value>>,
  pub(crate) format_specific: Option<Value>,
}
