#! /bin/bash

startdate=`echo '('$(date +"%s.%N") ' * 1000000)/1000' | bc`
enddate=`echo "($startdate + 4000)"|bc`

if [ -z "$DOMAIN" ]
then
  DOMAIN=localhost
fi


echo "Create root CA & Private key"
openssl req -x509 \
            -sha256 -days 356 \
            -nodes \
            -newkey rsa:2048 \
            -subj "/CN=${DOMAIN}/C=US/L=San Fransisco" \
            -keyout /certs/rootCA.key -out /certs/rootCA.crt


echo "Generate Private key"
openssl genrsa -out /certs/${DOMAIN}.key 2048

echo "Create csr conf"
cat > csr.conf <<EOF
[ req ]
default_bits = 2048
prompt = no
default_md = sha256
req_extensions = req_ext
distinguished_name = dn

[ dn ]
C = US
ST = California
L = San Fransisco
O = MLopsHub
OU = MlopsHub Dev
CN = ${DOMAIN}

[ req_ext ]
subjectAltName = @alt_names

[ alt_names ]
DNS.1 = ${DOMAIN}
DNS.2 = www.${DOMAIN}
IP.1 = 192.168.1.5
IP.2 = 192.168.1.6

EOF

echo "create CSR request using private key"
openssl req -new -key /certs/${DOMAIN}.key -out /certs/${DOMAIN}.csr -config csr.conf

echo "Create a external config file for the certificate"
cat > cert.conf <<EOF

authorityKeyIdentifier=keyid,issuer
basicConstraints=CA:FALSE
keyUsage = digitalSignature, nonRepudiation, keyEncipherment, dataEncipherment
subjectAltName = @alt_names

[alt_names]
DNS.1 = ${DOMAIN}

EOF

echo "Create SSl with self signed CA"
openssl x509 -req \
    -in /certs/${DOMAIN}.csr \
    -CA /certs/rootCA.crt -CAkey /certs/rootCA.key \
    -CAcreateserial -out /certs/${DOMAIN}.crt \
    -days 365 \
    -sha256 -extfile cert.conf
