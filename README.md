# Nimbus

A simple, lightweight web server written in Rust with zero external dependencies.

Made for student and courses about internet infrastructure. If you use this in production, you are cray cray.

```
 _   _ _           _
| \ | (_)_ __ ___ | |__  _   _ ___
|  \| | | '_ ` _ \| '_ \| | | / __|
| |\  | | | | | | | |_) | |_| \__ \
|_| \_|_|_| |_| |_|_.__/ \__,_|___/
                   a simple web server
```

## Features

- Serves static files from `/var/www`
- Optional PHP support
- Thread pool — handles up to 8 concurrent requests without spawning unbounded threads
- Runs in the background — start and stop with simple commands
- Install as a systemd service to start automatically on boot
- Request logging to `/var/log/nimbus.log`
- Production mode that hides error details from visitors
- Zero external dependencies — built entirely on Rust's standard library

## Installation

### Download the binary (recommended)

No Rust or compiler needed — just download and run.

**Linux x86-64** (regular PC / server):
```bash
sudo curl -L https://github.com/m-weiderstal/Nimbus/releases/latest/download/nimbus-linux-x86_64 \
  -o /usr/local/bin/nimbus && sudo chmod +x /usr/local/bin/nimbus
```

**Linux ARM64** (Raspberry Pi 4 / Pi 5):
```bash
sudo curl -L https://github.com/m-weiderstal/Nimbus/releases/latest/download/nimbus-linux-arm64 \
  -o /usr/local/bin/nimbus && sudo chmod +x /usr/local/bin/nimbus
```

### Build from source

Requires [Rust](https://rustup.rs).

```bash
cargo build --release
sudo cp target/release/nimbus /usr/local/bin/nimbus
```

### PHP support (optional)

Only needed if you use the `--php` flag:

```bash
sudo apt install php-cli
```

## Usage

```bash
nimbus [-ip <address>] [-port <number>] [--php] [--env production]
```

### Start the server

```bash
# Serve static HTML on all interfaces
nimbus -ip 0.0.0.0 -port 8080

# With PHP support
nimbus -ip 0.0.0.0 -port 8080 --php

# Production mode — hides error details from visitors
nimbus -ip 0.0.0.0 -port 8080 --php --env production
```

If `-ip` or `-port` are not provided, Nimbus will prompt for them.

### Stop the server

```bash
nimbus stop
```

### Auto-start on boot (systemd)

```bash
# Install as a service
sudo nimbus install -ip 0.0.0.0 -port 8080

# Remove from startup
sudo nimbus uninstall
```

## Commands

| Command | Description |
|---|---|
| `nimbus stop` | Stop the running server |
| `nimbus status` | Show whether Nimbus is running and on which address |
| `nimbus logs` | Follow the request log live |
| `nimbus show-ip` | Show the IP address of this machine |
| `nimbus install` | Install as a systemd service that starts on boot |
| `nimbus uninstall` | Remove from startup |
| `nimbus proxy` | Start the reverse proxy |
| `nimbus help` | Show help |

## Options

| Flag | Description |
|---|---|
| `-ip <address>` | IP to listen on. Use `0.0.0.0` for all interfaces, `127.0.0.1` for local only |
| `-port <number>` | Port to listen on (e.g. `8080`) |
| `--root <path>` | Directory to serve files from (default: `/var/www`) |
| `--php` | Enable PHP support. Without this flag, `.php` files return 404 |
| `--env production` | Hide error details from visitors |

## Logging

Requests are logged to `/var/log/nimbus.log` in the format:

```
[2026-05-11 14:23:01] 192.168.1.42 GET /index.html 200
[2026-05-11 14:23:04] 192.168.1.42 GET /missing.html 404
```

To set up the log file without root:

```bash
sudo touch /var/log/nimbus.log
sudo chown $USER /var/log/nimbus.log
```

Then follow it live:

```bash
nimbus logs
```

## Web root

Put your files in `/var/www`. Nimbus will serve `index.html` (or `index.php` with `--php`) for directory requests.

```bash
sudo mkdir -p /var/www
echo "<h1>Hello from Nimbus</h1>" | sudo tee /var/www/index.html
```

## Reverse proxy

Nimbus has a built-in host-based reverse proxy. This lets you run multiple sites on one machine, with the proxy routing each domain to its own backend server.

```
Browser → Nimbus proxy (port 80) → Nimbus site1 (port 8080) → /var/www/site1
                                  → Nimbus site2 (port 8081) → /var/www/site2
```

### Setup

**1. Create a routes config file** at `/etc/nimbus/routes.conf`:

```
# host          backend
site1.com        127.0.0.1:8080
site2.com        127.0.0.1:8081
```

**2. Create web roots and start a backend for each site:**

```bash
sudo mkdir -p /var/www/site1 /var/www/site2

nimbus -ip 127.0.0.1 -port 8080 --root /var/www/site1
nimbus -ip 127.0.0.1 -port 8081 --root /var/www/site2
```

**3. Start the proxy on port 80:**

```bash
sudo nimbus proxy -ip 0.0.0.0 -port 80 -config /etc/nimbus/routes.conf
```

On startup the proxy prints the loaded routes:

```
  route: site1.com → 127.0.0.1:8080
  route: site2.com → 127.0.0.1:8081
Proxy listening on http://0.0.0.0:80
```

The proxy automatically injects an `X-Real-IP` header with the visitor's IP address before forwarding each request to the backend.

### `--root` flag

By default Nimbus serves files from `/var/www`. Use `--root` to serve from a different directory:

```bash
nimbus -ip 127.0.0.1 -port 8080 --root /var/www/site1
```

## Architecture

Nimbus uses a fixed thread pool of 8 workers. Incoming connections are distributed via a channel — if all workers are busy, new connections queue up instead of spawning new threads. This prevents resource exhaustion under high load.

PHP support works by spawning a `php` process per request and returning its output as HTML. This is simple but not suited for very high traffic — for that, a FastCGI (PHP-FPM) setup would be more appropriate.
