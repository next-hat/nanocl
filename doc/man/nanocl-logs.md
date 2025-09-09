---
title: Logs
sidebar_position: 36
---

# Logs

# NAME

logs - Get logs of a process

## SYNOPSIS

**logs** \[**-s **\] \[**-u **\] \[**-t **\] \[**--timestamps**\] \[**-f
**\] \[**-h**\|**--help**\] \<*NAME*\>

## DESCRIPTION

Get logs of a process

## OPTIONS

**-s** *\<SINCE\>*  
Only include logs since unix timestamp

**-u** *\<UNTIL\>*  
Only include logs until unix timestamp

**-t** *\<TAIL\>*  
If integer only return last n logs, if "all" returns all logs

**--timestamps**  
Bool, if set include timestamp to ever log line

**-f**  
Bool, if set open the log as stream

**-h**, **--help**  
Print help

\<*NAME*\>  
Name of process to show logs
