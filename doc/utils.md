# Usefull command

## lsns - list namespaces

```sh
lsns
```

##  nsenter - run program in different namespaces

```sh
sudo nsenter -t 12267 -n ss -ltu
```

## Generate a nanocld client

```sh
docker run --rm -v $(pwd):/local openapitools/openapi-generator-cli generate -g rust -i /local/specs/v1/swagger.json -o /local/client
```

## Generate ssl cert from certbot

```sh
nanocl exec system-nano-proxy -- certbot --nginx --email email@email.com --agree-tos -d your-domain.com
```
