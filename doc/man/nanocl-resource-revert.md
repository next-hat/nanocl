---
title: Resource revert
sidebar_position: 52
---

# Resource revert

# NAME

revert - Revert a resource to a specific history

## SYNOPSIS

**revert** \[**-h**\|**--help**\] \<*NAME*\> \<*KEY*\>

## DESCRIPTION

Revert a resource to a specific history.

The restored resource is printed using the configured YAML, JSON, or TOML format.
TOML cannot represent all resource data, including `Data: null` or nested JSON
null values. If printing fails after a successful revert, the command reports
that the revert succeeded and returns an output error. It does not change formats
or omit data. Do not repeat the revert to retrieve its output; inspect the restored
resource with `nanocl resource inspect NAME --display json` or
`nanocl resource inspect NAME --display yaml`.

## OPTIONS

**-h**, **--help**  
Print help

\<*NAME*\>  
The name of the resource to revert

\<*KEY*\>  
The key of the history to revert to
