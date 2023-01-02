NAME
====

nanocl - The Self-Sufficient Hybrid-Cloud Orchestrator CLI

SYNOPSIS
========

**nanocl** \[**-H**\|**\--host**\] \[**-h**\|**\--help**\]
\[**-V**\|**\--version**\] \<*subcommands*\>

DESCRIPTION
===========

The Self-Sufficient Hybrid-Cloud Orchestrator CLI

OPTIONS
=======

**-H**, **\--host**=*HOST* \[default: unix://run/nanocl/nanocl.sock\]

:   Nanocld host

**-h**, **\--help**

:   Print help information

**-V**, **\--version**

:   Print version information

SUBCOMMANDS
===========

nanocl-namespace(1)

:   Manage namespaces

nanocl-cluster(1)

:   Manage clusters

nanocl-cargo(1)

:   Manage cargoes

nanocl-apply(1)

:   Apply a configuration file

nanocl-revert(1)

:   Revert a configuration file

nanocl-run(1)

:   Run a cargo in given environement

nanocl-exec(1)

:   Run a command in a running cargo instance

nanocl-proxy(1)

:   Manage proxy rules

nanocl-controller(1)

:   Manage nanocl controllers

nanocl-setup(1)

:   Setup given host to run nanocl

nanocl-version(1)

:   Show nanocl version information

nanocl-help(1)

:   Print this message or the help of the given subcommand(s)

VERSION
=======

v0.1.8
