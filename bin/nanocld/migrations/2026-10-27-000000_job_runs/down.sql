ALTER TABLE jobs DROP CONSTRAINT jobs_current_run_fk;
ALTER TABLE jobs DROP COLUMN current_run_key;
DROP TABLE job_run_steps;
DROP TABLE job_runs;
ALTER TABLE jobs DROP COLUMN incarnation_key;
