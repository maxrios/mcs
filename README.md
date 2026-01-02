# Max's Chat Service (MCS)

## Setting up Encryption (TLS)
This project uses TLS 1.2/1.3 for secure communication. Before running the application, you must generate self-signed certificates.

### Step 1: Create Config File
Create a file named localhost.cnf in the tls directory of this project to ensure the certificate works for localhost.
```bash
[req]
default_bits = 2048
prompt = no
default_md = sha256
distinguished_name = dn
req_extensions = req_ext

[dn]
CN = localhost

[req_ext]
subjectAltName = @alt_names

[alt_names]
DNS.1 = localhost
IP.1 = 127.0.0.1
```

### Step 2: Generate Keys and Certificates:
```bash


mkdir tls && cd tls

openssl genrsa -out ca.key 2048

openssl req -x509 -new -nodes -key ca.key -sha256 -days 1825 -out ca.cert -subj "/CN=MyChatRoot"

openssl genrsa -out server.key 2048

openssl req -new -key server.key -out server.csr -config localhost.cnf

openssl x509 -req -in server.csr -CA ca.cert -CAkey ca.key -CAcreateserial \
    -out server.cert -days 1825 -sha256 \
    -extensions req_ext -extfile localhost.cnf
```
