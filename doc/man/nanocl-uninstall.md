---
title: Uninstall
sidebar_position: 75
---

# Uninstall

# NAME

uninstall - Uninstall components

## SYNOPSIS

**uninstall** \[**--docker-host**\] \[**-t**\|**--template**\]
\[**--docker-desktop**\] \[**--json**\] \[**-h**\|**--help**\]

## DESCRIPTION

Uninstall components

Show per-cargo progress and a counted operation summary with elapsed times,
using the same display as `nanocl state rm`. Missing containers are reported
as unchanged. When output is redirected, print completed items and the
summary to stderr.

## OPTIONS

**--docker-host** *\<DOCKER_HOST\>*  
The docker host where nanocl is installed default is
unix:///var/run/docker.sock

**-t**, **--template** *\<TEMPLATE\>*  
Uninstall template to use for nanocl by default its detected

**--docker-desktop**  
Specify if the docker host is docker desktop detected if docker context
is desktop-linux

**--json**

Emit newline-delimited JSON (NDJSON) to stdout for tools and dashboards.
Records use `schema_version: 1` and `operation: "uninstall"`, with `state`,
`item`, and final `result` events matching Statefile remove output.
The `statefile` field contains the template when **--template** is supplied.

**-h**, **--help**  
Print help
