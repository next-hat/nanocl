/// Standard lib
use std::collections::HashMap;
use chrono::Utc;
/// Imported lib
use tabled::Tabled;
use chrono::DateTime;
use clap::{Parser, Subcommand};
use serde::{Serialize, Deserialize};

use super::utils::serde::*;
use super::utils::tabled::*;

#[derive(Debug, Parser)]
pub struct ContainerImageRemoveOpts {
  /// id or name of image to delete
  pub(crate) name: String,
}

#[derive(Debug, Parser, Serialize, Deserialize)]
pub struct ContainerImagePartial {
  pub(crate) name: String,
}

#[derive(Debug, Parser)]
pub struct ContainerImageDeployOpts {
  pub(crate) name: String,
}

#[derive(Debug, Subcommand)]
pub enum ContainerImageCommands {
  #[clap(alias("ls"))]
  List,
  Create(ContainerImagePartial),
  #[clap(alias("rm"))]
  Remove(ContainerImageRemoveOpts),
  #[clap(alias("dp"))]
  Deploy(ContainerImageDeployOpts),
}

/// Manage container images
#[derive(Debug, Parser)]
pub struct ContainerImageArgs {
  #[clap(subcommand)]
  pub(crate) commands: ContainerImageCommands,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressDetail {
  #[serde(rename = "current")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub current: Option<i64>,

  #[serde(rename = "total")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub total: Option<i64>,
}

#[derive(
  Debug, Tabled, Clone, Default, PartialEq, Eq, Serialize, Deserialize,
)]
pub struct ContainerImageSummary {
  #[serde(rename = "Id")]
  #[tabled(display_with = "display_sha_id")]
  pub id: String,

  #[serde(rename = "ParentId")]
  #[tabled(skip)]
  pub parent_id: String,

  #[serde(rename = "RepoTags")]
  #[serde(deserialize_with = "deserialize_nonoptional_vec")]
  #[tabled(display_with = "display_repo_tags")]
  pub repo_tags: Vec<String>,

  #[serde(rename = "RepoDigests")]
  #[serde(deserialize_with = "deserialize_nonoptional_vec")]
  #[tabled(skip)]
  pub repo_digests: Vec<String>,

  #[serde(rename = "Created")]
  #[tabled(display_with = "display_timestamp")]
  pub created: i64,

  #[serde(rename = "Size")]
  #[tabled(display_with = "display_size")]
  pub size: i64,

  #[serde(rename = "SharedSize")]
  #[tabled(skip)]
  pub shared_size: i64,

  #[serde(rename = "VirtualSize")]
  #[tabled(skip)]
  pub virtual_size: i64,

  #[serde(rename = "Labels")]
  #[serde(deserialize_with = "deserialize_nonoptional_map")]
  #[tabled(skip)]
  pub labels: HashMap<String, String>,

  #[serde(rename = "Containers")]
  #[tabled(skip)]
  pub containers: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateImageStreamInfo {
  #[serde(rename = "id")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub id: Option<String>,

  #[serde(rename = "error")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<String>,

  #[serde(rename = "status")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub status: Option<String>,

  #[serde(rename = "progress")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub progress: Option<String>,

  #[serde(rename = "progressDetail")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub progress_detail: Option<ProgressDetail>,
}

/// A test to perform to check that the container is healthy.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthConfig {
  /// The test to perform. Possible values are:  - `[]` inherit healthcheck from image or parent image - `[\"NONE\"]` disable healthcheck - `[\"CMD\", args...]` exec arguments directly - `[\"CMD-SHELL\", command]` run command with system's default shell
  #[serde(rename = "Test")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub test: Option<Vec<String>>,

  /// The time to wait between checks in nanoseconds. It should be 0 or at least 1000000 (1 ms). 0 means inherit.
  #[serde(rename = "Interval")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub interval: Option<i64>,

  /// The time to wait before considering the check to have hung. It should be 0 or at least 1000000 (1 ms). 0 means inherit.
  #[serde(rename = "Timeout")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub timeout: Option<i64>,

  /// The number of consecutive failures needed to consider a container as unhealthy. 0 means inherit.
  #[serde(rename = "Retries")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub retries: Option<i64>,

  /// Start period for the container to initialize before starting health-retries countdown in nanoseconds. It should be 0 or at least 1000000 (1 ms). 0 means inherit.
  #[serde(rename = "StartPeriod")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub start_period: Option<i64>,
}

/// Configuration for a container that is portable between hosts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContainerConfig {
  /// The hostname to use for the container, as a valid RFC 1123 hostname.
  #[serde(rename = "Hostname")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub hostname: Option<String>,

  /// The domain name to use for the container.
  #[serde(rename = "Domainname")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub domainname: Option<String>,

  /// The user that commands are run as inside the container.
  #[serde(rename = "User")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub user: Option<String>,

  /// Whether to attach to `stdin`.
  #[serde(rename = "AttachStdin")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub attach_stdin: Option<bool>,

  /// Whether to attach to `stdout`.
  #[serde(rename = "AttachStdout")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub attach_stdout: Option<bool>,

  /// Whether to attach to `stderr`.
  #[serde(rename = "AttachStderr")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub attach_stderr: Option<bool>,

  /// An object mapping ports to an empty object in the form:  `{\"<port>/<tcp|udp|sctp>\": {}}`
  #[serde(rename = "ExposedPorts")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub exposed_ports: Option<HashMap<String, HashMap<(), ()>>>,

  /// Attach standard streams to a TTY, including `stdin` if it is not closed.
  #[serde(rename = "Tty")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub tty: Option<bool>,

  /// Open `stdin`
  #[serde(rename = "OpenStdin")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub open_stdin: Option<bool>,

  /// Close `stdin` after one attached client disconnects
  #[serde(rename = "StdinOnce")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub stdin_once: Option<bool>,

  /// A list of environment variables to set inside the container in the form `[\"VAR=value\", ...]`. A variable without `=` is removed from the environment, rather than to have an empty value.
  #[serde(rename = "Env")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub env: Option<Vec<String>>,

  /// Command to run specified as a string or an array of strings.
  #[serde(rename = "Cmd")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub cmd: Option<Vec<String>>,

  #[serde(rename = "Healthcheck")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub healthcheck: Option<HealthConfig>,

  /// Command is already escaped (Windows only)
  #[serde(rename = "ArgsEscaped")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub args_escaped: Option<bool>,

  /// The name (or reference) of the image to use when creating the container, or which was used when the container was created.
  #[serde(rename = "Image")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub image: Option<String>,

  /// An object mapping mount point paths inside the container to empty objects.
  #[serde(rename = "Volumes")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub volumes: Option<HashMap<String, HashMap<(), ()>>>,

  /// The working directory for commands to run in.
  #[serde(rename = "WorkingDir")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub working_dir: Option<String>,

  /// The entry point for the container as a string or an array of strings.  If the array consists of exactly one empty string (`[\"\"]`) then the entry point is reset to system default (i.e., the entry point used by docker when there is no `ENTRYPOINT` instruction in the `Dockerfile`).
  #[serde(rename = "Entrypoint")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub entrypoint: Option<Vec<String>>,

  /// Disable networking for the container.
  #[serde(rename = "NetworkDisabled")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub network_disabled: Option<bool>,

  /// MAC address of the container.
  #[serde(rename = "MacAddress")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub mac_address: Option<String>,

  /// `ONBUILD` metadata that were defined in the image's `Dockerfile`.
  #[serde(rename = "OnBuild")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub on_build: Option<Vec<String>>,

  /// User-defined key/value metadata.
  #[serde(rename = "Labels")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub labels: Option<HashMap<String, String>>,

  /// Signal to stop a container as a string or unsigned integer.
  #[serde(rename = "StopSignal")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub stop_signal: Option<String>,

  /// Timeout to stop a container in seconds.
  #[serde(rename = "StopTimeout")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub stop_timeout: Option<i64>,

  /// Shell for when `RUN`, `CMD`, and `ENTRYPOINT` uses a shell.
  #[serde(rename = "Shell")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub shell: Option<Vec<String>>,
}

/// Information about an image in the local image cache.
#[derive(Debug, Serialize, Deserialize)]
pub struct ContainerImageInspect {
  /// ID is the content-addressable ID of an image.  This identified is a content-addressable digest calculated from the image's configuration (which includes the digests of layers used by the image).  Note that this digest differs from the `RepoDigests` below, which holds digests of image manifests that reference the image.
  #[serde(rename = "Id")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub id: Option<String>,

  /// List of image names/tags in the local image cache that reference this image.  Multiple image tags can refer to the same imagem and this list may be empty if no tags reference the image, in which case the image is \"untagged\", in which case it can still be referenced by its ID.
  #[serde(rename = "RepoTags")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub repo_tags: Option<Vec<String>>,

  /// List of content-addressable digests of locally available image manifests that the image is referenced from. Multiple manifests can refer to the same image.  These digests are usually only available if the image was either pulled from a registry, or if the image was pushed to a registry, which is when the manifest is generated and its digest calculated.
  #[serde(rename = "RepoDigests")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub repo_digests: Option<Vec<String>>,

  /// ID of the parent image.  Depending on how the image was created, this field may be empty and is only set for images that were built/created locally. This field is empty if the image was pulled from an image registry.
  #[serde(rename = "Parent")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub parent: Option<String>,

  /// Optional message that was set when committing or importing the image.
  #[serde(rename = "Comment")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub comment: Option<String>,

  /// Date and time at which the image was created, formatted in [RFC 3339](https://www.ietf.org/rfc/rfc3339.txt) format with nano-seconds.
  #[serde(rename = "Created")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub created: Option<String>,

  /// The ID of the container that was used to create the image.  Depending on how the image was created, this field may be empty.
  #[serde(rename = "Container")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub container: Option<String>,

  #[serde(rename = "ContainerConfig")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub container_config: Option<ContainerConfig>,

  /// The version of Docker that was used to build the image.  Depending on how the image was created, this field may be empty.
  #[serde(rename = "DockerVersion")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub docker_version: Option<String>,

  /// Name of the author that was specified when committing the image, or as specified through MAINTAINER (deprecated) in the Dockerfile.
  #[serde(rename = "Author")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub author: Option<String>,

  #[serde(rename = "Config")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub config: Option<ContainerConfig>,

  /// Hardware CPU architecture that the image runs on.
  #[serde(rename = "Architecture")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub architecture: Option<String>,

  /// CPU architecture variant (presently ARM-only).
  #[serde(rename = "Variant")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub variant: Option<String>,

  /// Operating System the image is built to run on.
  #[serde(rename = "Os")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub os: Option<String>,

  /// Operating System version the image is built to run on (especially for Windows).
  #[serde(rename = "OsVersion")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub os_version: Option<String>,

  /// Total size of the image including all layers it is composed of.
  #[serde(rename = "Size")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub size: Option<i64>,

  /// Total size of the image including all layers it is composed of.  In versions of Docker before v1.10, this field was calculated from the image itself and all of its parent images. Docker v1.10 and up store images self-contained, and no longer use a parent-chain, making this field an equivalent of the Size field.  This field is kept for backward compatibility, but may be removed in a future version of the API.
  #[serde(rename = "VirtualSize")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub virtual_size: Option<i64>,

  #[serde(rename = "GraphDriver")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub graph_driver: Option<GraphDriverData>,

  #[serde(rename = "RootFS")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub root_fs: Option<ImageInspectRootFs>,

  #[serde(rename = "Metadata")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub metadata: Option<ImageInspectMetadata>,
}

/// Additional metadata of the image in the local cache. This information is local to the daemon, and not part of the image itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageInspectMetadata {
  /// Date and time at which the image was last tagged in [RFC 3339](https://www.ietf.org/rfc/rfc3339.txt) format with nano-seconds.  This information is only available if the image was tagged locally, and omitted otherwise.
  #[serde(rename = "LastTagTime")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub last_tag_time: Option<DateTime<Utc>>,
}

/// Information about the image's RootFS, including the layer IDs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageInspectRootFs {
  #[serde(rename = "Type")]
  pub typ: String,

  #[serde(rename = "Layers")]
  #[serde(skip_serializing_if = "Option::is_none")]
  pub layers: Option<Vec<String>>,
}

/// Information about the storage driver used to store the container's and image's filesystem.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphDriverData {
  /// Name of the storage driver.
  #[serde(rename = "Name")]
  pub name: String,

  /// Low-level storage metadata, provided as key/value pairs.  This information is driver-specific, and depends on the storage-driver in use, and should be used for informational purposes only.
  #[serde(rename = "Data")]
  #[serde(deserialize_with = "deserialize_nonoptional_map")]
  pub data: HashMap<String, String>,
}
