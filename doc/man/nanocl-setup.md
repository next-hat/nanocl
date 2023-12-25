NAME
====

nanocl - \`nanocl install\` available options

SYNOPSIS
========

**nanocl** \[**\--docker-host**\] \[**\--docker-desktop**\]
\[**\--state-dir**\] \[**\--conf-dir**\] \[**\--gateway**\]
\[**\--advertise-addr**\] \[**\--deamon-hosts**\] \[**\--group**\]
\[**\--hostname**\] \[**-t**\|**\--template**\]
\[**-p**\|**\--force-pull**\] \[**-h**\|**\--help**\]

DESCRIPTION
===========

\`nanocl install\` available options

OPTIONS
=======

**\--docker-host**=*DOCKER\_HOST*

:   The docker host to install nanocl default is
    unix:///var/run/docker.sock

**\--docker-desktop**

:   Specify if the docker host is docker desktop detected if docker
    context is desktop-linux

**\--state-dir**=*STATE\_DIR*

:   The state directory to store the state of the nanocl daemon default
    is /var/lib/nanocl

**\--conf-dir**=*CONF\_DIR*

:   The configuration directory to store the configuration of the nanocl
    daemon default is /etc/nanocl

**\--gateway**=*GATEWAY*

:   The gateway address to use for the nanocl daemon default is detected

**\--advertise-addr**=*ADVERTISE\_ADDR*

:   The hosts to use for the nanocl daemon default is detected

**\--deamon-hosts**=*DEAMON\_HOSTS*

:   The hosts to use for the nanocl daemon default is detected

**\--group**=*GROUP*

:   The group to use for the nanocl daemon default is nanocl

**\--hostname**=*HOSTNAME*

:   The hostname to use for the nanocl daemon default is detected

**-t**, **\--template**=*TEMPLATE*

:   Installation template to use for nanocl by default its detected

**-p**, **\--force-pull**

:   Force repull of the nanocl components

**-h**, **\--help**

:   Print help
