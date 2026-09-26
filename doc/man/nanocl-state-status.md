---
title: State status
sidebar_position: 72
---

# State status

# NAME

status - Show running instances, health, and recent failures from a Statefile

## SYNOPSIS

**status** \[**-s**\|**--source** *SOURCE*\] \[**--watch**\]
\[**-h**\|**--help**\] \[**--** *ARGS*\]

## DESCRIPTION

Show the cargoes, virtual machines, jobs, and resources declared in a Statefile
together in a timestamped text table. Running counts show running processes out
of observed processes, not desired replicas. Cargo counts include init,
sandbox, and sidecar processes. Status shows actual and wanted state, with
scheduled jobs and present resources identified separately.

Health reflects the daemon's last report, including unknown or not reported
values. A resource's existence alone does not mean it is healthy. Recent
failure shows the latest reported error, failure, or unhealthy event matching
the element's kind and key, or an observed process failure, within the last
24 hours and no earlier than the current element's creation. Resource
synchronization failure reasons are unavailable if the daemon did not record
an event. Generic failure notifications use the available process exit or
health-check reason when it is more informative.

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

**-h**, **--help**\
Print help

\[*ARGS*\]\
Additional arguments to pass to the Statefile template

## EXAMPLES

```sh
nanocl state status -s Statefile.yml
nanocl state status -s Statefile.yml --watch
nanocl state status -s Statefile.yml --watch -- --name example
```
