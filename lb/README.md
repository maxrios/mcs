# MCS Load Balancer

A custom Layer 4 (TCP) Load Balancer written in Rust for the MCS architecture. This service replaces Nginx to provide dynamic, round-robin load balancing with active service discovery.

## Features

* **Layer 4 TCP Proxying:** Agnostic to the application protocol. Seamlessly handles the MCS binary protocol and TLS passthrough.
* **Dynamic Service Discovery:** Automatically detects new server instances as they scale up or down using Redis as a registry.
* **Round-Robin Scheduling:** Distributes incoming client connections evenly across available healthy backends.
* **Fault Tolerance:** Automatically stops routing traffic to dead nodes when their heartbeat expires.

## Architecture

### Service Registry Pattern
Unlike static load balancers that require hardcoded upstream IPs, this LB uses a **Service Registry** pattern backed by Redis:

1.  **Registration:** Each `server` instance identifies its own Docker container IP and publishes a heartbeat key to Redis (`mcs:node:<IP>:<PORT>`) every 2 seconds with a 5-second TTL.
2.  **Discovery:** The load balancer runs a background task that scans Redis for active `mcs:node:*` keys.
3.  **Routing:** It also maintains a local, thread-safe list of active backends. When a client connects, it picks a backend via Round-Robin and bridges the TCP streams.

If a server crashes, it stops sending heartbeats. Redis expires the key within 5 seconds, and the load balancer removes it from the rotation, preventing traffic from being sent to a dead node.

## Configuration

The service is configured via environment variables:

| Variable | Description | Default |
| :--- | :--- | :--- |
| `MCS_PORT` | The public port to listen on | `64400` |
| `REDIS_URL` | Connection string for the Service Registry | `redis://127.0.0.1:6379` |

## Development

### Prerequisites
* Rust (latest stable)
* A running Redis instance

### Running Locally
To run the load balancer standalone (assuming you have a local Redis and local MCS servers running):

```bash
cargo run -p load-balancer
