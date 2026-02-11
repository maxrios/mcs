# MCS Load Balancer

A TCP load balancer that serves as the secure edge gateway for MCS, handling TLS termination, service discovery, and traffic distribution.

## Features

* **TLS Termination:** Decrypts incoing traffic using `rustls` before forwarding MC proto packets to the chat service.
* **Least Connections:** Routes new clients to the backend with the fewest active sockets.
* **Service Discovery:** Polls a redis sorted set (`mcs:node`) to discover active chat service jobs dynamically.
* **Active Health Checks:** Periodically attempts to restablish connections chat service jobs and automatically offloads traffic from unhealthy nodes.

## Configuration

The load balancer is configured via environment variables:

| Variable | Description | Default |
| :--- | :--- | :--- |
| `MCS_PORT` | The public port to listen on for chat service traffic. | `64400` |
| `REDIS_URL` | Connection string for the shared Redis instance. | `redis://redis:6379` |
| `PROMETHEUS_PORT` | The public port to listen on for Prometheus metrics.  | `9000` |

## Certificates

The load balancer requires a valid certificate and private key to start. These must be placed in the `tls/` directory, relative to the binary:

* `tls/server.cert`: The full certificate chain (PEM format).
* `tls/server.key`: The private key (PKCS#1, PKCS#8, or SEC1 formats supported).

## Metrics (Prometheus)

The load balancer exposes a Prometheus-compatible metrics endpoint at `http://0.0.0.0:9000/metrics`.

### Key Metrics
* `lb_active_connections`: Total number of clients currently connected to the load balancer.
* `lb_backend_active_connections{backend="..."}`: Number of connections currently routed to a specific backend.
* `lb_backend_health_check_failures{backend="..."}`: Counter of failed health checks. A spike indicates a backend is down or unreachable.
* `lb_total_connections`: Cumulative count of all connections handled since startup.

## Development

### Running Locally
Ensure you have a Redis instance running and certificates generated.

```bash
# 1. Generate Dev Certs (if missing)
# See root README.md for OpenSSL commands

# 2. Run
export REDIS_URL=redis://127.0.0.1:6379
cargo run
```
