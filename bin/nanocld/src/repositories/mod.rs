/// Functions helper to manipulate database models.
///
/// Generic functions to manipulate database models.
pub(crate) mod generic;
/// Manage nodes table
pub(crate) mod node;
/// Manage metrics table
pub(crate) mod metric;
/// Manage HTTP metrics table
pub(crate) mod http_metric;
/// Manage stream metrics table
pub(crate) mod stream_metric;
/// Manage namespaces table
pub(crate) mod namespace;
/// Manage cargoes table
pub(crate) mod cargo;
/// Manage cargo_specs table
pub(crate) mod cargo_spec;
/// Manage vms table
pub(crate) mod vm;
/// Manage vm_specs table
pub(crate) mod vm_spec;
/// Manage vm_images table
pub(crate) mod vm_image;
/// Manage resources table
pub(crate) mod resource;
/// Manage resource_kinds table
pub(crate) mod resource_kind;
/// Manage resource_specs table
pub(crate) mod resource_spec;
/// Manage secrets table
pub(crate) mod secret;
/// Manage job table
pub(crate) mod job;
/// Manage container_instances table
pub(crate) mod container_instance;
