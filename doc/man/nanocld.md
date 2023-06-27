# NAME

Nanocl - Nanocl daemon Self Sufficient Hybrid Cloud Orchestrator

# SYNOPSIS

**Nanocl** \[**\--init**\] \[**-H**\|**\--hosts**\]
\[**\--docker-host**\] \[**\--state-dir**\] \[**\--config-dir**\]
\[**-h**\|**\--help**\] \[**-V**\|**\--version**\]

# DESCRIPTION

Nanocl daemon Self Sufficient Hybrid Cloud Orchestrator

# OPTIONS

**\--init**=_INIT_

: Ensure state is inited

**-H**, **\--hosts**=_HOSTS_

: Hosts to listen to use tcp:// and unix:// \[default:
unix:///run/nanocl.sock\]

**\--docker-host**=_DOCKER_HOST_

: Docker daemon socket to connect \[default: unix:///var/run/docker.sock\]

**\--state-dir**=_STATE_DIR_

: State directory \[default: /var/lib/nanocl\]

**\--config-dir**=_CONFIG_DIR_ \[default: /etc/nanocl\]

: Config directory

**-h**, **\--help**

: Print help

**-V**, **\--version**

: Print version

# VERSION

v0.1.19

# AUTHORS

nexthat team \<team\@next-hat.com\>
