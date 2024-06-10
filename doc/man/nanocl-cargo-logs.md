---
title: Cargo logs
sidebar_position: 9
---

# Cargo logs

## SYNOPSIS

**logs** \[**-s **\] \[**-u **\] \[**-t **\] \[**--timestamps**\] \[**-f
**\] \[**-h**\|**--help**\] \<*NAME*\>

## DESCRIPTION

Show logs

## OPTIONS

**-s**=*SINCE*  
Only include logs since unix timestamp

**-u**=*UNTIL*  
Only include logs until unix timestamp

**-t**=*TAIL*  
If integer only return last n logs, if "all" returns all logs

**--timestamps**  
Bool, if set include timestamp to ever log line

**-f**  
Bool, if set open the log as stream

**-h**, **--help**  
Print help

\<*NAME*\>  
Name of cargo to show logs
