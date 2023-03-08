#!/bin/sh

curl https://github.com/swagger-api/swagger-ui/archive/refs/tags/v4.14.3.tar.gz -L -o /tmp/swagger-ui.tar.gz -s

tar -xf /tmp/swagger-ui.tar.gz

mv ./swagger-ui-4.14.3/dist ./swagger-ui

rm -rf ./swagger-ui-4.14.3
