#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use bollard_next::models::ContainerCreateBody as Config;

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

/// Stored revision of a Job declaration, including immutable revision metadata.
/// Its containers run in sequence as a single unit to act like a command.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct JobSpec {
  /// Unique identifier of the immutable Job specification
  pub key: uuid::Uuid,
  /// Job resource key (Jobs are identified by their global name)
  pub job_key: String,
  /// Version of the specification
  pub version: String,
  /// Creation date of the specification revision
  pub created_at: chrono::NaiveDateTime,
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

/// A job and its current runtime state.
#[derive(Debug, Default, Clone, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct Job {
  /// Retained identity of this creation of the named job
  pub incarnation_key: uuid::Uuid,
  /// Immutable authored specification used by the live job
  pub spec_key: uuid::Uuid,
  /// Most recently accepted execution, if any
  pub current_run_key: Option<uuid::Uuid>,
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
        "IncarnationKey": job.incarnation_key,
        "SpecKey": job.spec_key,
        "RunKey": job.current_run_key,
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
  /// Retained identity of this creation of the named job
  pub incarnation_key: uuid::Uuid,
  /// Immutable authored specification used by the live job
  pub spec_key: uuid::Uuid,
  /// Most recently accepted execution, if any
  pub current_run_key: Option<uuid::Uuid>,
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
  /// Retained identity of this creation of the named job
  pub incarnation_key: uuid::Uuid,
  /// Immutable authored specification used by the live job
  pub spec_key: uuid::Uuid,
  /// Most recently accepted execution, if any
  pub current_run_key: Option<uuid::Uuid>,
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

/// Durable execution state. Unknown remains active until runtime evidence
/// confirms that no work belonging to this run is still executing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum JobRunState {
  Pending,
  Running,
  Stopping,
  Succeeded,
  Failed,
  Interrupted,
  Unknown,
}

impl JobRunState {
  pub fn is_terminal(self) -> bool {
    matches!(self, Self::Succeeded | Self::Failed | Self::Interrupted)
  }
}

/// Observed state of one ordered container step in a Job execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum JobStepState {
  Pending,
  Creating,
  Running,
  Succeeded,
  Failed,
  Interrupted,
  Unknown,
  Skipped,
}

/// A retained execution of an exact Job incarnation and authored revision.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct JobRun {
  pub key: uuid::Uuid,
  pub job_name: String,
  pub incarnation_key: uuid::Uuid,
  pub spec_key: uuid::Uuid,
  pub operation_key: uuid::Uuid,
  pub accepted_at: chrono::NaiveDateTime,
  pub started_at: Option<chrono::NaiveDateTime>,
  pub finished_at: Option<chrono::NaiveDateTime>,
  pub state: JobRunState,
  pub reason: Option<String>,
}

/// Retained result of one step; a missing exit code means it was not observed.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct JobRunStep {
  pub run_key: uuid::Uuid,
  /// Zero-based position in the accepted definition
  pub step_index: i32,
  /// Immutable Docker container ID, when container creation was observed
  pub container_id: Option<String>,
  pub state: JobStepState,
  pub started_at: Option<chrono::NaiveDateTime>,
  pub finished_at: Option<chrono::NaiveDateTime>,
  pub exit_code: Option<i64>,
  pub error: Option<String>,
}

/// Public initiating control operation, excluding internal ownership and locks.
/// Its completion acknowledges dispatch, not successful workload completion.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct JobRunOperation {
  pub key: uuid::Uuid,
  pub incarnation_key: uuid::Uuid,
  pub resource_kind: String,
  pub resource_key: String,
  pub action: String,
  pub spec_key: uuid::Uuid,
  pub accepted_at: chrono::NaiveDateTime,
  pub completed_at: Option<chrono::NaiveDateTime>,
  pub outcome: Option<String>,
  #[cfg_attr(feature = "utoipa", schema(value_type = Option<Any>))]
  pub diagnostic: Option<serde_json::Value>,
}

/// Execution history, including the authored definition before secret injection.
/// Retaining this history does not retain Docker logs.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "PascalCase"))]
pub struct JobRunInspect {
  pub run: JobRun,
  pub spec: JobSpec,
  pub operation: JobRunOperation,
  /// Results in ascending step index order
  pub steps: Vec<JobRunStep>,
}

/// Bounded Job execution history, newest accepted run first.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct JobHistoryQuery {
  /// Page size, defaults to 50 and must be between 1 and 100
  pub limit: Option<i64>,
  /// Number of records to skip, defaults to zero
  pub offset: Option<i64>,
}

impl JobHistoryQuery {
  pub fn validate(&self) -> Result<(i64, i64), String> {
    let limit = self.limit.unwrap_or(50);
    let offset = self.offset.unwrap_or(0);
    if !(1..=100).contains(&limit) {
      return Err("History limit must be between 1 and 100".into());
    }
    if offset < 0 {
      return Err("History offset must not be negative".into());
    }
    Ok((limit, offset))
  }
}

/// Optional identity fence for deleting the exact Job inspected by a caller.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct JobDeleteQuery {
  /// Expected incarnation. Omission preserves ordinary deletion by name.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub incarnation: Option<uuid::Uuid>,
  /// Expected current run. With an incarnation and no run, require no run.
  #[cfg_attr(
    feature = "serde",
    serde(skip_serializing_if = "Option::is_none")
  )]
  pub run: Option<uuid::Uuid>,
}

impl JobDeleteQuery {
  pub fn validate(&self) -> Result<(), String> {
    if self.run.is_some() && self.incarnation.is_none() {
      return Err("A Job run deletion fence requires an incarnation".into());
    }
    Ok(())
  }
}

#[cfg(all(test, feature = "serde"))]
mod tests {
  use super::{
    JobDeleteQuery, JobHistoryQuery, JobPartial, JobRun, JobRunInspect,
    JobRunOperation, JobRunState, JobRunStep, JobSpec, JobStepState,
  };
  use crate::statefile::Statefile;

  fn job_with_schedule(schedule: &str) -> JobPartial {
    JobPartial {
      name: "scheduled-job".into(),
      schedule: Some(schedule.into()),
      ..Default::default()
    }
  }

  #[test]
  fn job_spec_revision_metadata_stays_out_of_authored_input() {
    let key = uuid::Uuid::from_u128(1);
    let created_at = chrono::DateTime::UNIX_EPOCH.naive_utc();
    let spec = JobSpec {
      key,
      job_key: "backup".into(),
      version: "v0.19".into(),
      created_at,
      name: "backup".into(),
      secrets: Some(vec!["database-credentials".into()]),
      metadata: Some(serde_json::json!({"team": "storage"})),
      schedule: Some("@daily".into()),
      ttl: Some(60),
      image_pull_secret: Some("registry-credentials".into()),
      image_pull_policy: Some(crate::generic::ImagePullPolicy::IfNotPresent),
      containers: vec![super::Config {
        image: Some("busybox".into()),
        cmd: Some(vec!["true".into()]),
        ..Default::default()
      }],
    };
    let revision = serde_json::to_value(&spec).unwrap();
    assert_eq!(revision["Key"], key.to_string());
    assert_eq!(revision["JobKey"], "backup");
    assert_eq!(revision["Version"], "v0.19");
    assert_eq!(
      revision["CreatedAt"],
      serde_json::to_value(created_at).unwrap()
    );
    assert_eq!(
      serde_json::from_value::<JobSpec>(revision.clone()).unwrap(),
      spec
    );

    let partial = JobPartial::from(spec);
    let authored = serde_json::to_value(&partial).unwrap();
    let mut declaration = revision.clone();
    for field in ["Key", "JobKey", "Version", "CreatedAt"] {
      assert!(authored.get(field).is_none());
      declaration.as_object_mut().unwrap().remove(field);
      let mut supplied = authored.clone();
      supplied[field] = revision[field].clone();
      assert!(serde_json::from_value::<JobPartial>(supplied).is_err());
    }
    assert_eq!(authored, declaration);
    assert_eq!(
      serde_json::from_value::<JobPartial>(authored.clone()).unwrap(),
      partial
    );
    assert!(serde_json::from_value::<JobSpec>(authored).is_err());
  }

  #[test]
  fn job_deletion_query_preserves_no_run_and_rejects_unscoped_run() {
    let incarnation = uuid::Uuid::from_u128(1);
    let run = uuid::Uuid::from_u128(2);
    let never_started = JobDeleteQuery {
      incarnation: Some(incarnation),
      run: None,
    };
    assert!(never_started.validate().is_ok());
    assert_eq!(
      serde_json::to_value(never_started).unwrap(),
      serde_json::json!({
        "incarnation": incarnation,
      })
    );
    let exact = JobDeleteQuery {
      incarnation: Some(incarnation),
      run: Some(run),
    };
    assert_eq!(
      serde_json::to_value(&exact).unwrap(),
      serde_json::json!({
        "incarnation": incarnation, "run": run,
      })
    );
    assert_eq!(
      serde_json::from_value::<JobDeleteQuery>(
        serde_json::to_value(&exact).unwrap()
      )
      .unwrap(),
      exact
    );
    assert!(JobDeleteQuery::default().validate().is_ok());
    assert!(
      JobDeleteQuery {
        incarnation: None,
        run: Some(run)
      }
      .validate()
      .is_err()
    );
  }

  #[test]
  fn job_history_pagination_is_bounded_and_serializes_query_names() {
    assert_eq!(JobHistoryQuery::default().validate().unwrap(), (50, 0));
    let query = JobHistoryQuery {
      limit: Some(100),
      offset: Some(10),
    };
    assert_eq!(query.validate().unwrap(), (100, 10));
    assert_eq!(
      serde_json::to_value(query).unwrap(),
      serde_json::json!({
        "limit": 100, "offset": 10,
      })
    );
    for query in [
      JobHistoryQuery {
        limit: Some(0),
        offset: None,
      },
      JobHistoryQuery {
        limit: Some(101),
        offset: None,
      },
      JobHistoryQuery {
        limit: None,
        offset: Some(-1),
      },
    ] {
      assert!(query.validate().is_err());
    }
  }

  #[test]
  fn job_history_keeps_unknown_work_active_and_exit_codes_nullable() {
    for state in [
      JobRunState::Pending,
      JobRunState::Running,
      JobRunState::Stopping,
      JobRunState::Unknown,
    ] {
      assert!(!state.is_terminal());
    }
    for state in [
      JobRunState::Succeeded,
      JobRunState::Failed,
      JobRunState::Interrupted,
    ] {
      assert!(state.is_terminal());
    }
    let step = JobRunStep {
      run_key: uuid::Uuid::from_u128(1),
      step_index: 0,
      container_id: None,
      state: JobStepState::Unknown,
      started_at: None,
      finished_at: None,
      exit_code: None,
      error: Some("Unable to observe runtime".into()),
    };
    let json = serde_json::to_value(&step).unwrap();
    assert_eq!(json["State"], "Unknown");
    assert!(json["ExitCode"].is_null());
    assert!(json["ContainerId"].is_null());
    assert_eq!(serde_json::from_value::<JobRunStep>(json).unwrap(), step);
  }

  #[test]
  fn job_history_detail_roundtrips_pinned_definition_and_safe_operation() {
    let key = uuid::Uuid::from_u128(1);
    let accepted_at = chrono::DateTime::UNIX_EPOCH.naive_utc();
    let history = JobRunInspect {
      run: JobRun {
        key,
        job_name: "backup".into(),
        incarnation_key: key,
        spec_key: key,
        operation_key: key,
        accepted_at,
        started_at: None,
        finished_at: None,
        state: JobRunState::Pending,
        reason: None,
      },
      spec: JobSpec {
        key,
        job_key: "backup".into(),
        version: "v0.19".into(),
        created_at: accepted_at,
        name: "backup".into(),
        secrets: Some(vec!["database-credentials".into()]),
        ..Default::default()
      },
      operation: JobRunOperation {
        key,
        incarnation_key: key,
        resource_kind: "Job".into(),
        resource_key: "backup".into(),
        action: "start".into(),
        spec_key: key,
        accepted_at,
        completed_at: Some(accepted_at),
        outcome: Some("completed".into()),
        diagnostic: None,
      },
      steps: vec![],
    };
    let value = serde_json::to_value(&history).unwrap();
    assert_eq!(value["Run"]["State"], "Pending");
    assert_eq!(value["Operation"]["Outcome"], "completed");
    assert_eq!(
      value["Spec"]["Secrets"],
      serde_json::json!(["database-credentials"])
    );
    let operation = value["Operation"].as_object().unwrap();
    assert_eq!(operation.len(), 10);
    for private in ["OwnerBootId", "NodeName", "LockDevice", "LockInode"] {
      assert!(!operation.contains_key(private));
    }
    assert_eq!(
      serde_json::from_value::<JobRunInspect>(value).unwrap(),
      history
    );
  }

  #[test]
  fn job_events_identify_the_exact_incarnation_and_run() {
    let incarnation_key = uuid::Uuid::from_u128(1);
    let spec_key = uuid::Uuid::from_u128(2);
    let run_key = uuid::Uuid::from_u128(3);
    let actor = crate::system::EventActor::from(super::Job {
      incarnation_key,
      spec_key,
      current_run_key: Some(run_key),
      spec: JobSpec {
        key: spec_key,
        job_key: "backup".into(),
        version: "v0.19".into(),
        created_at: chrono::DateTime::UNIX_EPOCH.naive_utc(),
        name: "backup".into(),
        ..Default::default()
      },
      ..Default::default()
    });
    assert_eq!(actor.key.as_deref(), Some("backup"));
    let attributes = actor.attributes.unwrap();
    assert_eq!(attributes["IncarnationKey"], incarnation_key.to_string());
    assert_eq!(attributes["SpecKey"], spec_key.to_string());
    assert_eq!(attributes["RunKey"], run_key.to_string());
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
