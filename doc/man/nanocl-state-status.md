---
title: State status
sidebar_position: 72
---

# State status

# NAME

status - Show running instances, health, and recent failures from a Statefile

## SYNOPSIS

**status** \[**-s**\|**--source** *SOURCE*\] \[**--watch**\]
\[**--wide**\] \[**-h**\|**--help**\] \[**--** *ARGS*\]

## DESCRIPTION

Show the cargoes, virtual machines, jobs, and resources declared in a Statefile
together in a timestamped overview. The default columns are OBJECT, STATE, PROC,
and HEALTH. Output fits the terminal width, wrapping full object names without
shortening their identities. Narrow terminals use labelled fields beneath each
object. Job results, desired-state differences, recent failures, and read errors
appear beneath the affected object.

PROC shows running processes out of observed processes, not desired replicas or
readiness. Cargo counts include init, sandbox, and sidecar processes. States use
labels such as running, stopped, completed, and failed. Scheduled jobs and
present resources are identified separately. Desired state appears when it
differs, except for completed or failed job runs whose creation/start request
has already ended.

With --wide, show the detailed table with separate kind and name columns,
actual/wanted state, job results, and full UTC failure timestamps. This table
may exceed the terminal width.

Health reflects the daemon's last report, including unknown or not reported
values. A resource's existence alone does not mean it is healthy. Recent
failure shows the latest reported error, failure, or unhealthy event matching
the element's kind and key, or an observed process failure, within the last
24 hours and no earlier than the current element's creation. Resource
synchronization failure reasons are unavailable if the daemon did not record
an event. Generic failure notifications use the available process exit or
health-check reason when it is more informative.
The compact overview shows failure age separately from current state and
health, so a recovered object can still have a recent failure.

Missing elements remain in the table. Unavailable status or failure history is
distinguished from a successful read with no failure found. Without --watch,
daemon read errors print the available results and exit with failure; watch
mode retries on subsequent refreshes.

The Statefile and its recursive SubStates are rendered once with the supplied
template arguments. Status reads the resulting declarations without applying
changes or running Statefile secrets that execute commands. Namespaces follow
each rendered Statefile. Runtime reads use the configured CLI daemon, as with
state apply.

With --watch, refresh the overview every two seconds until Ctrl-C. Interactive
terminal output is cleared for each refresh unless TERM is dumb; redirected
output appends plain timestamped snapshots. The rendered Statefile is reused
for every refresh. Restart the command after changing the Statefile or its
template arguments.

## OPTIONS

**-s**, **--source** *\<SOURCE\>*\
Path or URL to the Statefile. Uses the same default source lookup as state apply.

**--watch**\
Refresh status every two seconds until Ctrl-C.

**--wide**\
Show detailed columns and full UTC failure timestamps without fitting the table
to the terminal width.

**-h**, **--help**\
Print help

\[*ARGS*\]\
Additional arguments to pass to the Statefile template

## EXAMPLES

```sh
nanocl state status -s Statefile.yml
nanocl state status -s Statefile.yml --watch
nanocl state status -s Statefile.yml --wide
nanocl state status -s Statefile.yml --watch -- --name example
```
