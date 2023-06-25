#!/bin/sh

set -e

## Test if the 403, and 502 pages are present
if [ ! -f /usr/share/nginx/html/403.html ]; then
  echo "Using default 403"
  cp /html/403.html /usr/share/nginx/html/403.html
fi

## Test if the 502 page is present
if [ ! -f /usr/share/nginx/html/502.html ]; then
  echo "Using default 502"
  cp /html/502.html /usr/share/nginx/html/502.html
fi

## Test if the default index.html is present
if [ ! -f /usr/share/nginx/html/index.html ]; then
  echo "Using default index"
  cp /html/index.html /usr/share/nginx/html/index.html
fi

nginx -g "daemon off;"
