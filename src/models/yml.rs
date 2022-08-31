use std::collections::HashMap;
use serde::{Serialize, Deserialize};

use crate::models::ClusterJoinPartial;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct YmlCargo {
  pub(crate) name: String,
  #[serde(rename(deserialize = "image"))]
  pub(crate) image_name: String,
  pub(crate) dns_entry: Option<String>,
  pub(crate) domainname: Option<String>,
  pub(crate) hostname: Option<String>,
  pub(crate) binds: Option<Vec<String>>,
  pub(crate) replicas: Option<i32>,
  pub(crate) environnements: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct YmlNetwork {
  pub(crate) name: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct YmlGitRepository {
  pub(crate) name: String,
  pub(crate) url: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct YmlCluster {
  pub(crate) name: String,
  pub(crate) auto_start: Option<bool>,
  pub(crate) proxy_templates: Option<Vec<String>>,
  #[serde(rename(deserialize = "vars"))]
  pub(crate) variables: Option<HashMap<String, String>>,
  pub(crate) joins: Option<Vec<ClusterJoinPartial>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct YmlNamespaceConfig {
  // name of the namespace
  pub(crate) name: String,
  // list of cargo to deploy
  pub(crate) cargoes: Vec<YmlCargo>,
  // list of network to create when deploy
  pub(crate) networks: Vec<YmlNetwork>,
  // List of configuration a bit like github workflow matrix
  pub(crate) clusters: Vec<YmlCluster>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum YmlConfigTypes {
  #[serde(rename(deserialize = "namespace"))]
  Namespace,
  #[serde(rename(deserialize = "cargo"))]
  Cargo,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct YmlFile {
  #[serde(rename(deserialize = "type"))]
  pub(crate) file_type: YmlConfigTypes,
}
