openssl req -x509 -newkey rsa:4096 -keyout ca.key -out ca.crt -days 365 -nodes -subj "/CN=NanoclCA"

openssl req -newkey rsa:4096 -keyout server.key -out server.csr -nodes -subj "/CN=*"

openssl x509 -req -in server.csr -out server.crt -CA ca.crt -CAkey ca.key -CAcreateserial -days 365

openssl req -newkey rsa:4096 -keyout client.key -out client.csr -nodes -subj "/CN=NanoclClient"

openssl x509 -req -in client.csr -out client.crt -CA ca.crt -CAkey ca.key -CAcreateserial -days 365
