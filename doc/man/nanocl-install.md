---
title: Install
sidebar_position: 28
---

# Install

# NAME

install - Install components

## SYNOPSIS

**install** \[**--docker-host**\] \[**--docker-desktop**\]
\[**--state-dir**\] \[**--conf-dir**\] \[**--gateway**\]
\[**--advertise-addr**\] \[**--daemon-hosts**\] \[**--group**\]
\[**--hostname**\] \[**-t**\|**--template**\]
\[**-p**\|**--force-pull**\] \[**-f **\] \[**--json**\]
\[**-h**\|**--help**\]

## DESCRIPTION

Install components

Show per-cargo progress, image download progress, and a counted operation
summary with elapsed times, using the same display as `nanocl state apply`.
When output is redirected, print completed items and the summary to stderr.

## OPTIONS

**--docker-host** *\<DOCKER_HOST\>*  
The docker host to install nanocl default is unix:///var/run/docker.sock

**--docker-desktop**  
Specify if the docker host is docker desktop detected if docker context
is desktop-linux

**--state-dir** *\<STATE_DIR\>*  
The state directory to store the state of the nanocl daemon default is
/var/lib/nanocl

**--conf-dir** *\<CONF_DIR\>*  
The configuration directory to store the configuration of the nanocl
daemon default is /etc/nanocl

**--gateway** *\<GATEWAY\>*  
The gateway address to use for the nanocl daemon default is detected

**--advertise-addr** *\<ADVERTISE_ADDR\>*  
The hosts to use for the nanocl daemon default is detected

**--daemon-hosts** *\<DAEMON_HOSTS\>*  
The hosts to use for the nanocl daemon default is detected

**--group** *\<GROUP\>*  
The group to use for the nanocl daemon default is nanocl

**--hostname** *\<HOSTNAME\>*  
The hostname to use for the nanocl daemon default is detected

**-t**, **--template** *\<TEMPLATE\>*  
Installation template to use for nanocl by default its detected

**-p**, **--force-pull**  
Force re pull of the nanocl components

**-f**  
Attach to the container logs after installation

**--json**

Emit newline-delimited JSON (NDJSON) to stdout for tools and dashboards.
Records use `schema_version: 1` and `operation: "install"`, with `state`,
`item`, `image`, and final `result` events matching Statefile apply output.
The `statefile` field contains the template when **--template** is supplied.
Cannot be combined with **-f**.

**-h**, **--help**  
Print help
