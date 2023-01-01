-- Your SQL goes here
CREATE TYPE "proxy_template_modes" AS ENUM ('http', 'stream');

CREATE TABLE "proxy_templates" (
  "name" VARCHAR NOT NULL UNIQUE PRIMARY KEY,
  "mode" proxy_template_modes NOT NUll,
  "content" TEXT NOT NULL
);

INSERT INTO "proxy_templates" ("name", "mode", "content") VALUES ('nodejs-single', 'http', 'server {
  server_name {{vars.pre_domain}}{{domain_name}};
  listen {{host_ip}}:80;
  location / {
      proxy_set_header upgrade $http_upgrade;
      proxy_set_header connection "upgrade";
      proxy_http_version 1.1;
      proxy_set_header x-forwarded-for $proxy_add_x_forwarded_for;
      proxy_set_header host $host;
      proxy_pass http://{{target_ip}}:{{target_port}};
  }

  if ($host != {{vars.pre_domain}}{{domain_name}}) {
      return 502;
  }
}
');

INSERT INTO "proxy_templates" ("name", "mode", "content") VALUES ('nodejs-single-ssl', 'http', 'server {
  server_name {{vars.pre_domain}}{{domain_name}};
  listen 443 ssl;
  location / {
      proxy_set_header upgrade $http_upgrade;
      proxy_set_header connection "upgrade";
      proxy_http_version 1.1;
      proxy_set_header x-forwarded-for $proxy_add_x_forwarded_for;
      proxy_set_header host $host;
      proxy_pass http://{{target_ip}}:{{target_port}};
  }

  if ($host != {{vars.pre_domain}}{{domain_name}}) {
      return 502;
  }

  ssl_certificate /etc/letsencrypt/live/{{vars.pre_domain}}{{domain_name}}/fullchain.pem;
  ssl_certificate_key /etc/letsencrypt/live/{{vars.pre_domain}}{{domain_name}}/privkey.pem;
  include /etc/letsencrypt/options-ssl-nginx.conf;
  ssl_dhparam /etc/letsencrypt/ssl-dhparams.pem;
}

server {
  if ($host = {{vars.pre_domain}}{{domain_name}}) {
    return 301 https://$host$request_uri;
  }

  server_name {{vars.pre_domain}}{{domain_name}};
  listen {{host_ip}};
  return 404;
}
');
