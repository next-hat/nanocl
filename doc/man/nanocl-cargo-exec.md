---
title: Cargo exec
sidebar_position: 5
---

# Cargo exec

## SYNOPSIS

**exec** \[**-t**\|**--tty**\] \[**--detach-keys**\] \[**-e **\]
\[**--privileged**\] \[**-u **\] \[**-w**\|**--workdir**\]
\[**-h**\|**--help**\] \<*NAME*\> \[*COMMAND*\]

## DESCRIPTION

Execute a command inside a cargo

## OPTIONS

**-t**, **--tty**  
Allocate a pseudo-TTY

**--detach-keys**=*DETACH_KEYS*  
Override the key sequence for detaching a container

**-e**=*ENV*  
Set environment variables

**--privileged**  
Give extended privileges to the command

**-u**=*USER*  
Username or UID (format: "\<name\|uid\>\[:\<group\|gid\>\]")

**-w**, **--workdir**=*WORKING_DIR*  
Working directory inside the container

**-h**, **--help**  
Print help

\<*NAME*\>  
Name of cargo to execute command

\[*COMMAND*\]  
Command to execute
