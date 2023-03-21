#!/bin/sh

set -e

ls -lR /html

## Test if the 403, and 502 pages are present
if [ ! -f /usr/share/nginx/html/403.html ]; then
  echo "Using default 403"
  cp /html/403.html /usr/share/nginx/html/403.html
fi

if [ ! -f /usr/share/nginx/html/502.html ]; then
  echo "Using default 502"
  cp /html/502.html /usr/share/nginx/html/502.html
fi

## Test if the default index.html is present
if [ ! -f /usr/share/nginx/html/index.html ]; then
  echo "Using default index"
  cp /html/index.html /usr/share/nginx/html/index.html
fi

## Remove tmp files
rm -f /html/403.html /html/502.html /html/index.html /html/default.conf

nginx -g "daemon off;"
