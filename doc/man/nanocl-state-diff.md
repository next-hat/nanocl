---
title: State diff
sidebar_position: 68
---

# State diff

# NAME

diff - Preview changes from a Statefile without applying them

## SYNOPSIS

**diff** \[**-s**\|**--source** *SOURCE*\] \[**-r**\|**--reload**\]
\[**--remove-orphans**\] \[**--json**\]
\[**--pager**\|**--no-pager**\] \[**-h**\|**--help**\] \[**--** *ARGS*\]

## DESCRIPTION

Compare a rendered Statefile with the selected daemon's current configuration.
Print a unified YAML diff with surrounding context for changes to images, ports,
mounts, replicas, and resource configuration. Removed lines have a `-` prefix
and added lines have a `+` prefix. The diff compares normalized configuration,
so hunk line numbers do not refer to the source Statefile.

Sensitive fields are omitted without placeholders or notices. Unchanged
configuration and changes affecting only sensitive fields produce no text
output. Use --json for all actions, including unchanged elements, orphans,
reloads, and sensitive changes. Set `NO_COLOR=1` to disable terminal colors.
This command does not apply changes or require confirmation.

With terminal input and output and `TERM` other than `dumb`, text opens in
`less` and stays open until you press `q`, even for a short diff. Scroll with
arrow keys or PageUp/PageDown, search with `/`, and quit with `q`. Redirected or
piped output and JSON always print directly. If `less` is unavailable, text
prints directly. Empty diffs stay silent.

See [Preview Statefile changes](../state-diff.md) for comparison rules, JSON
output, and examples.

## OPTIONS

**-s**, **--source** *\<SOURCE\>*\
Path or URL to the Statefile. Uses the same default source lookup as state apply.

**-r**, **--reload**\
Preview updates to existing cargoes, virtual machines, and resources even when
their configuration is unchanged, matching state apply --reload. Use --json to
see updates without visible configuration changes.

**--remove-orphans**\
Preview removal of orphaned secrets, cargoes, virtual machines, and resources
only for sections explicitly present in the Statefile. An empty section selects
all its orphans for removal; an omitted section keeps them. Orphaned jobs are
always kept.

**--json**\
Print one JSON document directly, containing all items and a summary. Sensitive
changes retain their paths with `"[REDACTED]"` values and `redacted: true`.
Cannot be combined with --pager.

**--pager**\
Explicitly select the default behavior: keep even a short diff open in `less`
until you press `q`. Requires terminal input and output and `TERM` other than
`dumb`; redirected or piped output still prints directly. Cannot be combined
with --no-pager or --json.

**--no-pager**\
Print directly without opening a pager. Cannot be combined with --pager.

**-h**, **--help**\
Print help

\[*ARGS*\]\
Additional arguments to pass to the Statefile template

## EXAMPLES

```sh
nanocl state diff -s Statefile.yml
nanocl state diff -s Statefile.yml --remove-orphans
nanocl state diff -s Statefile.yml --json
nanocl state diff -s Statefile.yml --pager
nanocl state diff -s Statefile.yml --no-pager
nanocl state diff -s Statefile.yml > preview.diff
nanocl state diff -s Statefile.yml -- --name example
```
