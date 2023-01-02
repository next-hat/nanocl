NAME
====

nanocld - Nanocl daemon Self Sufficient Hybrid Cloud Orchestrator

SYNOPSIS
========

**nanocld** \[**-h**\|**\--help**\] \[**-V**\|**\--version**\]
\[**\--genopenapi**\] \[**\--install-components**\]
\[**-H**\|**\--host**\] \[**\--docker-host**\] \[**\--state-dir**\]
\[**\--config-dir**\] \[**\--github-user**\] \[**\--github-token**\]

DESCRIPTION
===========

Nanocl daemon Self Sufficient Hybrid Cloud Orchestrator

OPTIONS
=======

**-h**, **\--help**

:   Print help information

**-V**, **\--version**

:   Print version information

**\--genopenapi**

:   Only available if nanocld have been builded with feature openapi

**\--install-components**

:   Only install required components this have to be called after fresh
    installation

**-H**, **\--host**=*HOSTS*

:   Daemon host to listen to you can use tcp:// and unix:// \[default:
    unix:///run/nanocl/nanocl.sock\]

**\--docker-host**=*DOCKER\_HOST*

:   Docker daemon socket to connect \[default:
    unix:///run/docker.sock\]

**\--state-dir**=*STATE\_DIR*

:   State directory \[default: /var/lib/nanocl\]

**\--config-dir**=*CONFIG\_DIR* \[default: /etc/nanocl\]

:   Config directory

**\--github-user**=*GITHUB\_USER*

:   Github user used to make request with identity

**\--github-token**=*GITHUB\_TOKEN*

:   Generated token for given github user

VERSION
=======

v0.1.2
