---
title: Nanocl
sidebar_position: 93
---

# Nanocl

# NAME

nanocl - Container and virtual machine orchestrator

## SYNOPSIS

**nanocl** \[**-H**\|**--host**\] \[**-h**\|**--help**\]
\[**-V**\|**--version**\] \<*subcommands*\>

## DESCRIPTION

Nanocl is a modern, self-sufficient orchestrator for containers and
virtual machines. It delivers a clean dev→prod workflow with declarative
Statefiles (YAML/TOML/JSON) and opinionated, predictable defaults. Built
in Rust for performance and safety, it keeps operational overhead low
while remaining powerful and extensible. Manage cargoes, resources,
jobs, and VMs with dynamic routing, DNS, and end-to-end TLS. Start local
on a single node and scale out when ready — simple to learn,
production-grade by design.

## OPTIONS

**-H**, **--host** *\<HOST\>*  
Nanocld host default: unix://run/nanocl/nanocl.sock

**-h**, **--help**  
Print help (see a summary with -h)

**-V**, **--version**  
Print version

# SUBCOMMANDS

nanocl-namespace(1)  
Manage namespaces

nanocl-secret(1)  
Manage secrets

nanocl-job(1)  
Manage jobs

nanocl-cargo(1)  
Manage cargoes

nanocl-vm(1)  
Manage virtual machines

nanocl-resource(1)  
Manage resources

nanocl-metric(1)  
Manage metrics

nanocl-context(1)  
Manage contexts

nanocl-node(1)  
Manage nodes (experimental)

nanocl-state(1)  
Apply or Remove a Statefile

nanocl-event(1)  
Show or watch events

nanocl-ps(1)  
Show processes

nanocl-logs(1)  
Get logs of a process

nanocl-inspect(1)  
Inspect a process

nanocl-info(1)  
Show nanocl host information

nanocl-version(1)  
Show nanocl version information

nanocl-install(1)  
Install components

nanocl-uninstall(1)  
Uninstall components

nanocl-backup(1)  
Backup the current state

nanocl-stats(1)  
Stats of the process

nanocl-help(1)  
Print this message or the help of the given subcommand(s)

# VERSION

v0.17.0
