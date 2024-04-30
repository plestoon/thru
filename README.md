# Thru

Create QUIC/TCP/UDP tunnels for one another.

## Install

`cargo install thru`

## Example usage

TCP through QUIC.

### Server endpoint

Forward QUIC traffic to tcpbin echo server.

```shell
thru -t quic://0.0.0.0:4242==tcp://tcpbin.com:4242 --cert cert.pem --key key.pem
```

### Client endpoint

Forward TCP tranffic to QUIC tunnel.

```shell
thru -t tcp://127.0.0.1:4242==quic://example.com:4242
```

### Echo client

```shell
nc 127.0.0.1 4242
```

## TLS certificate and key

They are only needed for QUIC tunnels.

`--peer-cert` is for the client to specify the server's root certificate. It's only needed for self-signed certificates
and if it hasn't been installed on the system keystore.