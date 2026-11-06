---
title: State diff
sidebar_position: 68
---

# State diff

# NAME

diff - Preview changes from a Statefile without applying them

## SYNOPSIS

**diff** \[**-s**\|**--source** *SOURCE*\] \[**-r**\|**--reload**\]
\[**--remove-orphans**\] \[**--json**\|**--summary**\]
\[**--pager**\|**--no-pager**\] \[**-h**\|**--help**\] \[**--** *ARGS*\]

## DESCRIPTION

Compare a rendered Statefile with the selected daemon's current configuration.
Print a unified YAML diff with surrounding context for changes to images, ports,
mounts, replicas, and resource configuration. Removed lines have a `-` prefix
and added lines have a `+` prefix. The diff compares normalized configuration,
so hunk line numbers do not refer to the source Statefile.

Sensitive fields are omitted without placeholders or notices. Unchanged
configuration and changes affecting only sensitive fields produce no default
diff output. Use --summary for a compact list of planned actions and totals,
including unchanged elements, orphans, reloads, and sensitive changes without
their values. Use --json for the complete structured plan. Set `NO_COLOR=1` to
disable terminal colors. This command does not apply changes or require
confirmation.

Summary rows follow apply order and wrap to fit the terminal without shortening
object names. Reasons appear below the affected object, including why an orphan
is kept or removed. When no actions change the state, the summary explicitly
reports `No changes.`.

With terminal input and output and `TERM` other than `dumb`, text opens in
`less` and stays open until you press `q`, even for a short diff. Scroll with
arrow keys or PageUp/PageDown, search with `/`, and quit with `q`. Redirected or
piped output and JSON always print directly. If `less` is unavailable, text
prints directly. Empty default diffs stay silent; summaries report the plan
even when no configuration changes are visible.

See [Preview Statefile changes](../state-diff.md) for comparison rules, JSON
output, and examples.

## OPTIONS

**-s**, **--source** *\<SOURCE\>*\
Path or URL to the Statefile. Uses the same default source lookup as state apply.

**-r**, **--reload**\
Preview updates to existing cargoes, virtual machines, and resources even when
their configuration is unchanged, matching state apply --reload. Use --summary
or --json to see updates without visible configuration changes.

**--remove-orphans**\
Preview removal of orphaned secrets, cargoes, virtual machines, and resources
only for sections explicitly present in the Statefile. An empty section selects
all its orphans for removal; an omitted section keeps them. Orphaned jobs are
always kept.

**--json**\
Print one JSON document directly, containing all items and a summary. Sensitive
changes retain their paths with `"[REDACTED]"` values and `redacted: true`.
Cannot be combined with --pager or --summary.

**--summary**\
Print a compact list of planned create, update, remove, and unchanged actions
with totals. Include forced updates, changes to sensitive fields, and retained
or removed orphans without printing configuration values. Cannot be combined
with --json.

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
nanocl state diff -s Statefile.yml --summary --no-pager
nanocl state diff -s Statefile.yml --pager
nanocl state diff -s Statefile.yml --no-pager
nanocl state diff -s Statefile.yml > preview.diff
nanocl state diff -s Statefile.yml -- --name example
```
