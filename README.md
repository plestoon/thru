# Thru

A QUIC tunnel for TCP/UDP.

## Install

`cargo install thru`

## Example usage

TCP through QUIC.

### Server endpoint

Forward QUIC traffic to tcpbin echo server.

```shell
thru -t quic://0.0.0.0:4242==tcp://tcpbin.com:4242 --cert certchain.pem --key key.pem
```

### Client endpoint

Forward TCP tranffic to QUIC tunnel.

```shell
thru -t tcp://127.0.0.1:4242==quic://example.com:4242 --peer-cert root.pem
```

### Echo client

```shell
nc 127.0.0.1 4242
```

## TLS certificate

On the client side, `--peer-cert` is for the client to specify the server's root certificate.
It's only needed for self-signed certificates and if it hasn't been installed on the system keystore.