---
title: VM revert
---

# VM revert

## NAME

revert - Restore a VM specification revision

## SYNOPSIS

**nanocl vm revert** <*KEY*> <*HISTORY_ID*>

## DESCRIPTION

Restore the complete specification selected from `nanocl vm history KEY`.
The key is canonical `namespace.name`; the history ID is a specification UUID
from the live VM's current incarnation. Revisions from another VM or an earlier
incarnation are rejected.

Revert uses the normal VM update lifecycle and waits for its matching operation
to complete. A running guest must shut down gracefully before runtime replacement.
The saved declaration is restored as a new revision; history is retained.
This restores configuration, not a snapshot of guest disk contents.

## OPTIONS

**-h**, **--help**
Print help

## EXAMPLE

`nanocl vm revert global.desktop SPEC_UUID`
