#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use bollard_next::container::Config;

use crate::{
  generic::ImagePullPolicy,
  process::Process,
  system::{EventActor, EventActorKind, ObjPsStatus},
};

#[cfg(feature = "utoipa")]
use super::generic::Any;

/// Job partial is used to create a new job
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(
  feature = "serde",
  serde(deny_unknown_fields, rename_all = "PascalCase")
)]
pub struct JobPartial {
  /// Name of the job
  pub name: String,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  /// Secrets to load as environment variables
  pub secrets: Option<Vec<String>>,
  /// Metadata (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// Five-field cron expression or supported shortcut; see
  /// [`crate::cron::validate_schedule`].
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub schedule: Option<String>,
  /// Remove the job after (x) seconds after execution
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub ttl: Option<usize>,
  /// Secret to use when pulling the image
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_secret: Option<String>,
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_policy: Option<ImagePullPolicy>,
  /// List of container to run
  pub containers: Vec<Config>,
}

impl JobPartial {
  /// Validate the schedule after any Statefile templates have been rendered.
  pub fn validate_schedule(&self) -> Result<(), String> {
    if let Some(schedule) = &self.schedule {
      crate::cron::validate_schedule(schedule)?;
    }
    Ok(())
  }
}

/// A job specification is a collection of containers to run in sequence as a
/// single unit to act like a command.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct JobSpec {
  /// Name of the job
  pub name: String,
  /// Secrets to load as environment variables
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub secrets: Option<Vec<String>>,
  /// Metadata (user defined)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  #[cfg_attr(feature = "utoipa", schema(value_type = HashMap<String, Any>))]
  pub metadata: Option<serde_json::Value>,
  /// Schedule of the job (cron)
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub schedule: Option<String>,
  /// Remove the job after (x) seconds after execution
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub ttl: Option<usize>,
  /// Secret to use when pulling the image
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_secret: Option<String>,
  /// Image pull policy
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub image_pull_policy: Option<ImagePullPolicy>,
  /// Containers to run
  pub containers: Vec<Config>,
}

impl From<JobSpec> for JobPartial {
  fn from(spec: JobSpec) -> Self {
    Self {
      name: spec.name,
      secrets: spec.secrets,
      metadata: spec.metadata,
      schedule: spec.schedule,
      ttl: spec.ttl,
      containers: spec.containers,
      image_pull_secret: spec.image_pull_secret,
      image_pull_policy: spec.image_pull_policy,
    }
  }
}

impl From<JobPartial> for JobSpec {
  fn from(spec: JobPartial) -> Self {
    Self {
      name: spec.name,
      secrets: spec.secrets,
      metadata: spec.metadata,
      schedule: spec.schedule,
      ttl: spec.ttl,
      containers: spec.containers,
      image_pull_secret: spec.image_pull_secret,
      image_pull_policy: spec.image_pull_policy,
    }
  }
}

/// A job and its current runtime state.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Job {
  /// When the job was created
  pub created_at: chrono::NaiveDateTime,
  /// When the job was updated
  pub updated_at: chrono::NaiveDateTime,
  /// Status of the job
  pub status: ObjPsStatus,
  /// Specification of the job
  pub spec: JobSpec,
}

/// Convert a job into a job partial.
impl From<Job> for JobPartial {
  fn from(job: Job) -> Self {
    job.spec.into()
  }
}

/// Convert a Job into an EventActor
impl From<Job> for EventActor {
  fn from(job: Job) -> Self {
    Self {
      key: Some(job.spec.name.clone()),
      kind: EventActorKind::Job,
      attributes: Some(serde_json::json!({
        "Name": job.spec.name,
        "Metadata": job.spec.metadata,
      })),
    }
  }
}

/// Summary of a job (used in list)
#[derive(Debug)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct JobSummary {
  /// When the job was created
  pub created_at: chrono::NaiveDateTime,
  /// When the job was updated
  pub updated_at: chrono::NaiveDateTime,
  /// Status of the job
  pub status: ObjPsStatus,
  /// Number of instances
  pub instance_total: usize,
  /// Number of instance that succeeded
  pub instance_success: usize,
  /// Number of instance running
  pub instance_running: usize,
  /// Number of instance failed
  pub instance_failed: usize,
  /// Specification of the job
  pub spec: JobSpec,
}

/// Detailed information about a job
#[derive(Clone, Debug)]
#[cfg_attr(feature = "test", derive(Default))]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct JobInspect {
  /// When the job was created
  pub created_at: chrono::NaiveDateTime,
  /// When the job was updated
  pub updated_at: chrono::NaiveDateTime,
  /// Status of the job
  pub status: ObjPsStatus,
  /// Number of instances
  pub instance_total: usize,
  /// Number of instance that succeeded
  pub instance_success: usize,
  /// Number of instance running
  pub instance_running: usize,
  /// Number of instance failed
  pub instance_failed: usize,
  /// Specification of the job
  pub spec: JobSpec,
  /// List of instances
  pub instances: Vec<Process>,
}

/// Convert a job inspect into a job partial
impl From<JobInspect> for JobPartial {
  fn from(job: JobInspect) -> Self {
    job.spec.into()
  }
}

#[cfg(all(test, feature = "serde"))]
mod tests {
  use super::JobPartial;
  use crate::statefile::Statefile;

  fn job_with_schedule(schedule: &str) -> JobPartial {
    JobPartial {
      name: "scheduled-job".into(),
      schedule: Some(schedule.into()),
      ..Default::default()
    }
  }

  fn assert_schedule_rejected(schedule: &str) {
    let job = job_with_schedule(schedule);
    let json = serde_json::to_string(&job).unwrap();
    let parsed = serde_json::from_str::<JobPartial>(&json).unwrap();
    assert!(parsed.validate_schedule().is_err(), "{json}");
    let yaml = serde_yaml::to_string(&job).unwrap();
    let parsed = serde_yaml::from_str::<JobPartial>(&yaml).unwrap();
    assert!(parsed.validate_schedule().is_err(), "{yaml}");
    let toml = toml::to_string(&job).unwrap();
    let parsed = toml::from_str::<JobPartial>(&toml).unwrap();
    assert!(parsed.validate_schedule().is_err(), "{toml}");
  }

  #[test]
  fn job_schedule_rejects_unsafe_values() {
    for schedule in [
      "* * * * *\n* * * * * injected-command",
      "* * * * *\r* * * * * injected-command",
      "* * * * *\r\nSHELL=/tmp/injected-shell",
      "*\t* * * *",
      "* * * * *\0",
      "* * * * *\u{000b}",
      "* * * * * injected-command",
      "* * * * *;injected-command",
    ] {
      assert_schedule_rejected(schedule);
    }
  }

  #[test]
  fn job_schedule_rejects_malformed_values() {
    for schedule in [
      "",
      "bad",
      "* * * *",
      "* * * * * *",
      "60 * * * *",
      "*/0 * * * *",
      "* * * * 5-8",
    ] {
      assert_schedule_rejected(schedule);
    }
  }

  #[test]
  fn job_schedule_accepts_omitted_or_null() {
    for json in [
      r#"{"Name":"job","Containers":[]}"#,
      r#"{"Name":"job","Containers":[],"Schedule":null}"#,
    ] {
      let job = serde_json::from_str::<JobPartial>(json).unwrap();
      assert_eq!(job.schedule, None);
      assert!(job.validate_schedule().is_ok());
    }
    for yaml in [
      "Name: job\nContainers: []\n",
      "Name: job\nContainers: []\nSchedule: null\n",
    ] {
      let job = serde_yaml::from_str::<JobPartial>(yaml).unwrap();
      assert_eq!(job.schedule, None);
      assert!(job.validate_schedule().is_ok());
    }
    let job =
      toml::from_str::<JobPartial>("Name = 'job'\nContainers = []").unwrap();
    assert_eq!(job.schedule, None);
    assert!(job.validate_schedule().is_ok());
  }

  #[test]
  fn job_schedule_roundtrips_valid_values() {
    for schedule in
      ["* * * * *", "*/15 9-17 * * 1-5", "0 0 1,15 * 0,6", "@daily"]
    {
      let job = job_with_schedule(schedule);
      let json = serde_json::to_string(&job).unwrap();
      let parsed = serde_json::from_str::<JobPartial>(&json).unwrap();
      assert_eq!(parsed, job);
      assert!(parsed.validate_schedule().is_ok());
      let yaml = serde_yaml::to_string(&job).unwrap();
      let parsed = serde_yaml::from_str::<JobPartial>(&yaml).unwrap();
      assert_eq!(parsed, job);
      assert!(parsed.validate_schedule().is_ok());
      let toml = toml::to_string(&job).unwrap();
      let parsed = toml::from_str::<JobPartial>(&toml).unwrap();
      assert_eq!(parsed, job);
      assert!(parsed.validate_schedule().is_ok());
    }
  }

  #[test]
  fn statefile_rejects_invalid_job_schedule() {
    let statefile = serde_json::json!({
      "ApiVersion": "v0.18",
      "Jobs": [job_with_schedule("* * * * *\n* * * * * injected-command")],
    });
    let json = serde_json::to_string(&statefile).unwrap();
    let parsed = serde_json::from_str::<Statefile>(&json).unwrap();
    assert!(parsed.jobs.unwrap()[0].validate_schedule().is_err());
    let yaml = serde_yaml::to_string(&statefile).unwrap();
    let parsed = serde_yaml::from_str::<Statefile>(&yaml).unwrap();
    assert!(parsed.jobs.unwrap()[0].validate_schedule().is_err());
    let toml = toml::to_string(&statefile).unwrap();
    let parsed = toml::from_str::<Statefile>(&toml).unwrap();
    assert!(parsed.jobs.unwrap()[0].validate_schedule().is_err());
  }

  #[test]
  fn job_schedule_preserves_templates_until_validation() {
    let yaml = "Name: job\nContainers: []\nSchedule: '${{ Args.Schedule }}'";
    let job = serde_yaml::from_str::<JobPartial>(yaml).unwrap();
    assert_eq!(job.schedule.as_deref(), Some("${{ Args.Schedule }}"));
    assert!(job.validate_schedule().is_err());
  }
}
