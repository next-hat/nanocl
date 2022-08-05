NAME
====

nanocl-cluster - Manage clusters

SYNOPSIS
========

**nanocl-cluster** \[**-h**\|**\--help**\] \[**\--namespace**\]
\<*subcommands*\>

DESCRIPTION
===========

Create, Update, Inspect or Delete cluster

OPTIONS
=======

**-h**, **\--help**

:   Print help information

**\--namespace**=*NAMESPACE*

:   Namespace to target by default global is used

SUBCOMMANDS
===========

nanocl-cluster-list(1)

:   List existing cluster

nanocl-cluster-create(1)

:   Create new cluster

nanocl-cluster-remove(1)

:   Remove cluster by its name

nanocl-cluster-start(1)

:   Start cluster by its name

nanocl-cluster-inspect(1)

:   Inspect cluster by its name

nanocl-cluster-nginx-template(1)

:   Control cluster nginx templates

nanocl-cluster-network(1)

:   Control cluster networks

nanocl-cluster-variable(1)

:   Control cluster variables

nanocl-cluster-join(1)

:   Create containers instances of a cargo inside given cluster and
    network

nanocl-cluster-help(1)

:   Print this message or the help of the given subcommand(s)
