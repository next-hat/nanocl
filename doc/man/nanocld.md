# NAME

Nanocl - Nanocl Daemon - Self Sufficient Hybrid Cloud Orchestrator

# SYNOPSIS

**Nanocl** \[**\--init**\] \[**-H**\|**\--hosts**\]
\[**\--docker-host**\] \[**\--state-dir**\] \[**\--conf-dir**\]
\[**\--gateway**\] \[**\--hostname**\] \[**\--node**\]
\[**\--advertise-addr**\] \[**\--gid**\] \[**-h**\|**\--help**\]
\[**-V**\|**\--version**\]

# DESCRIPTION

Nanocl Daemon - Self Sufficient Hybrid Cloud Orchestrator

# OPTIONS

**\--init**

:   Ensure state is inited

**-H**, **\--hosts**=*HOSTS*

:   Hosts to listen to use tcp:// and unix:// \[default:
    unix:///run/nanocl.sock\]

**\--docker-host**=*DOCKER_HOST*

:   Docker daemon socket to connect \[default:
    unix:///var/run/docker.sock\]

**\--state-dir**=*STATE_DIR*

:   State directory \[default: /var/lib/nanocl\]

**\--conf-dir**=*CONF_DIR* \[default: /etc/nanocl\]

:   Config directory

**\--gateway**=*GATEWAY*

:   Gateway automatically detected to host default source ip gateway if
    not set

**\--hostname**=*HOSTNAME*

:   Hostname to use for the node automatically detected if not set

**\--node**=*NODES*

:   Join current node to a cluster

**\--advertise-addr**=*ADVERTISE_ADDR*

:   Address to advertise to other nodes

**\--gid**=*GID* \[default: 0\]

:   Group id

**-h**, **\--help**

:   Print help

**-V**, **\--version**

:   Print version

# VERSION

v0.9.0

# AUTHORS

nexthat team \<team@next-hat.com\>
