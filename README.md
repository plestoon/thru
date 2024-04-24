# Thru
Create QUIC/TCP/UDP tunnels for one another.

## Example usage

TCP through QUIC.

### Server endpoint

Forward QUIC traffic to tcpbin echo server.

```shell
thru -t quic://0.0.0.0:4242==tcp://tcpbin.com:4242 --tls-cert-path cert.pem --tls-key-path key.pem
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