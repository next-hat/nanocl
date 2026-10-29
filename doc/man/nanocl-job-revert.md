---
title: Job revert
---

# Job revert

## NAME

revert - Restore a Job configuration revision without starting a run

## SYNOPSIS

**nanocl job revert** <*NAME*> <*HISTORY_ID*>

## DESCRIPTION

Restore a configuration selected from `nanocl job history NAME`. The history ID
is a specification UUID belonging to the same global Job name, including a
revision retained from an earlier incarnation. A live Job must exist.

Revert uses the existing Job replacement lifecycle: delete the selected live Job,
wait for deletion, and recreate its saved configuration and schedule. An active
run is stopped through the normal deletion path. Previous configurations and
execution records remain available. The replacement has a new incarnation and
revision and does not immediately execute; restored cron schedules can start
future runs. The name remains reserved until replacement finishes, preventing
concurrent creation from claiming it between deletion and recreation.

Replacement is not atomic. A creation failure after deletion leaves the old Job
removed and reports an error. Inspect the Job and retained history before retrying.

## OPTIONS

**-h**, **--help**
Print help

## EXAMPLE

`nanocl job revert backup SPEC_UUID`
