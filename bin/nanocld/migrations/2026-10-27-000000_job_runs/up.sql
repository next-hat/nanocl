-- Fresh-cluster schema: existing installations must be recreated from Statefiles.
ALTER TABLE jobs ADD COLUMN incarnation_key UUID NOT NULL REFERENCES resource_incarnations (key) ON DELETE RESTRICT;
CREATE TABLE job_runs (
  key UUID PRIMARY KEY,
  job_name TEXT NOT NULL,
  incarnation_key UUID NOT NULL REFERENCES resource_incarnations (key) ON DELETE RESTRICT,
  spec_key UUID NOT NULL REFERENCES specs (key) ON DELETE RESTRICT,
  operation_key UUID NOT NULL UNIQUE REFERENCES operations (key) ON DELETE RESTRICT,
  accepted_at TIMESTAMPTZ NOT NULL,
  started_at TIMESTAMPTZ,
  finished_at TIMESTAMPTZ,
  state TEXT NOT NULL CHECK (state IN ('Pending', 'Running', 'Stopping', 'Succeeded', 'Failed', 'Interrupted', 'Unknown')),
  reason TEXT,
  cancellation_requested BOOLEAN NOT NULL DEFAULT FALSE,
  UNIQUE (incarnation_key, key),
  CHECK ((finished_at IS NOT NULL) = (state IN ('Succeeded', 'Failed', 'Interrupted')))
);
CREATE UNIQUE INDEX job_runs_active_idx ON job_runs (incarnation_key) WHERE finished_at IS NULL;
CREATE INDEX job_runs_history_idx ON job_runs (job_name, accepted_at DESC, key DESC);
CREATE TABLE job_run_steps (
  run_key UUID NOT NULL REFERENCES job_runs (key) ON DELETE RESTRICT,
  step_index INT4 NOT NULL CHECK (step_index >= 0),
  container_id TEXT UNIQUE,
  state TEXT NOT NULL CHECK (state IN ('Pending', 'Creating', 'Running', 'Succeeded', 'Failed', 'Interrupted', 'Unknown', 'Skipped')),
  started_at TIMESTAMPTZ,
  finished_at TIMESTAMPTZ,
  exit_code BIGINT,
  error TEXT,
  PRIMARY KEY (run_key, step_index),
  CHECK (state <> 'Succeeded' OR (exit_code IS NOT NULL AND exit_code = 0)),
  CHECK (state NOT IN ('Pending', 'Creating', 'Skipped') OR exit_code IS NULL)
);
ALTER TABLE jobs ADD COLUMN current_run_key UUID;
ALTER TABLE jobs ADD CONSTRAINT jobs_current_run_fk FOREIGN KEY (incarnation_key, current_run_key) REFERENCES job_runs (incarnation_key, key) ON DELETE RESTRICT;
