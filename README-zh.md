# Thru

QUIC加密通道，支持TCP/UDP。

## 安装

`cargo install thru`

## 示例

TCP走QUIC通道

### 服务端

QUIC转TCP

```shell
thru -t quic://0.0.0.0:4242==tcp://tcpbin.com:4242 --cert certchain.pem --key key.pem
```

### 客户端

TCP转QUIC

```shell
thru -t tcp://127.0.0.1:4242==quic://example.com:4242 --peer-cert root.pem
```

### 测试echo服务

```shell
nc 127.0.0.1 4242
```

## TLS证书

由于QUIC使用了TLS加密，所以需要配置TLS证书。

服务端配置对应域名的证书及私钥。

对于自签名证书，客户端需要配置对应的根证书（`--peer-cert`）。如果已在系统中安装该证书，则不需要这个参数。

## github

https://github.com/plestoon/thru