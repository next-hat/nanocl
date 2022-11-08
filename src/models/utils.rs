pub mod serde {
  use std::collections::HashMap;
  use serde::{Deserializer, de::DeserializeOwned};

  pub fn deserialize_nonoptional_vec<
    'de,
    D: Deserializer<'de>,
    T: DeserializeOwned,
  >(
    d: D,
  ) -> Result<Vec<T>, D::Error> {
    serde::Deserialize::deserialize(d).map(|x: Option<_>| x.unwrap_or_default())
  }

  pub fn deserialize_nonoptional_map<
    'de,
    D: Deserializer<'de>,
    T: DeserializeOwned,
  >(
    d: D,
  ) -> Result<HashMap<String, T>, D::Error> {
    serde::Deserialize::deserialize(d).map(|x: Option<_>| x.unwrap_or_default())
  }
}

pub mod tabled {
  use chrono::{NaiveDateTime, DateTime, Utc};
  use super::super::{Port, ContainerSummaryNetworkSettings};

  pub fn optional_string(s: &Option<String>) -> String {
    match s {
      None => String::from(""),
      Some(s) => s.to_owned(),
    }
  }

  pub fn display_vec_string(o: &[String]) -> String {
    o.join(", ")
  }

  pub fn optional_container_name(s: &Option<Vec<String>>) -> String {
    match s {
      None => String::from(""),
      Some(s) => s
        .iter()
        .map(|s| s.replace('/', ""))
        .collect::<Vec<_>>()
        .join(", "),
    }
  }

  pub fn display_sha_id(id: &str) -> String {
    let no_sha = id.replace("sha256:", "");
    let (id, _) = no_sha.split_at(12);
    id.to_string()
  }

  pub fn display_timestamp(timestamp: &i64) -> String {
    // Create a NaiveDateTime from the timestamp
    let naive = NaiveDateTime::from_timestamp(*timestamp, 0);

    // Create a normal DateTime from the NaiveDateTime
    let datetime: DateTime<Utc> = DateTime::from_utc(naive, Utc);

    // Format the datetime how you want
    let newdate = datetime.format("%Y-%m-%d %H:%M:%S");
    newdate.to_string()
  }

  pub fn display_repo_tags(repos: &[String]) -> String {
    repos[0].to_string()
  }

  pub fn display_size(size: &i64) -> String {
    let result = *size as f64 * 1e-9;
    format!("{:.5} GB", result)
  }

  pub fn display_optional_ports(s: &Option<Vec<Port>>) -> String {
    match s {
      None => String::from(""),
      Some(ports) => ports.iter().fold(String::new(), |mut acc, port| {
        acc = format!(
          "{}{}:{} ",
          acc,
          port.public_port.unwrap_or_default(),
          port.private_port
        );
        acc
      }),
    }
  }

  pub fn display_container_summary_network_settings(
    s: &Option<ContainerSummaryNetworkSettings>,
  ) -> String {
    match s {
      None => String::from(""),
      Some(summary) => {
        if let Some(network) = &summary.networks {
          let mut ips = String::new();
          for key in network.keys() {
            let netinfo = network.get(key).unwrap();
            let ip = netinfo.ip_address.to_owned().unwrap_or_default();
            ips = format!("{}{} ", ips, ip,);
          }
          return ips;
        }
        String::from("")
      }
    }
  }
}
