NAME
====

nanocl-cluster - Manage clusters

SYNOPSIS
========

**nanocl-cluster** \[**\--namespace**\] \[**-h**\|**\--help**\]
\<*subcommands*\>

DESCRIPTION
===========

Create, Update, Inspect or Delete cluster

OPTIONS
=======

**\--namespace**=*NAMESPACE*

:   Namespace to target by default global is used

**-h**, **\--help**

:   Print help information (use \`-h\` for a summary)

SUBCOMMANDS
===========

nanocl-cluster-list(1)

:   List existing cluster

nanocl-cluster-create(1)

:   Create a new cluster

nanocl-cluster-remove(1)

:   Remove one cluster

nanocl-cluster-start(1)

:   Start one cluster

nanocl-cluster-inspect(1)

:   Display detailed information on one cluster

nanocl-cluster-network(1)

:   Manage cluster networks

nanocl-cluster-variable(1)

:   Manage cluster variables

nanocl-cluster-join(1)

:   Create cargo instances inside given cluster and network

nanocl-cluster-help(1)

:   Print this message or the help of the given subcommand(s)
