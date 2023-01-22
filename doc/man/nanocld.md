NAME
====

Nanocl - Nanocl daemon Self Sufficient Hybrid Cloud Orchestrator

SYNOPSIS
========

**Nanocl** \[**\--init**\] \[**-H**\|**\--hosts**\]
\[**\--docker-host**\] \[**\--state-dir**\] \[**\--config-dir**\]
\[**-h**\|**\--help**\] \[**-V**\|**\--version**\]

DESCRIPTION
===========

Nanocl daemon Self Sufficient Hybrid Cloud Orchestrator

OPTIONS
=======

**\--init**=*INIT*

:   Ensure state is inited

**-H**, **\--hosts**=*HOSTS*

:   Hosts to listen to use tcp:// and unix:// \[default:
    unix:///run/nanocl.sock\]

**\--docker-host**=*DOCKER\_HOST*

:   Docker daemon socket to connect \[default: unix:///run/docker.sock\]

**\--state-dir**=*STATE\_DIR*

:   State directory \[default: /var/lib/nanocl\]

**\--config-dir**=*CONFIG\_DIR* \[default: /etc/nanocl\]

:   Config directory

**-h**, **\--help**

:   Print help

**-V**, **\--version**

:   Print version

VERSION
=======

v0.1.19

AUTHORS
=======

nexthat team \<team\@next-hat.com\>
