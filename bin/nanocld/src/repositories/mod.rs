/// Functions helper to manipulate database models.
///
/// Generic functions to manipulate database models.
pub mod generic;
/// Manage nodes table
pub mod node;
/// Manage metrics table
pub mod metric;
/// Manage HTTP metrics table
pub mod http_metric;
/// Manage stream metrics table
pub mod stream_metric;
/// Manage namespaces table
pub mod namespace;
/// Manage cargoes table
pub mod cargo;
/// Manage cargo_configs table
pub mod cargo_spec;
/// Manage vms table
pub mod vm;
/// Manage vm_configs table
pub mod vm_spec;
/// Manage vm_images table
pub mod vm_image;
/// Manage resources table
pub mod resource;
/// Manage resource_kinds table
pub mod resource_kind;
/// Manage resource_configs table
pub mod resource_spec;
/// Manage secrets table
pub mod secret;
/// Manage job table
pub mod job;
