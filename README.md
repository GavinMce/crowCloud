<div align="center">

```
  ___ _ __ _____      _____ ___ _   _  _____  _ 
 / __| '__/ _ \ \ /\ / / __| | | |/ _ \ | | |
| (__| | | (_) \ V  V / (__| | |_| | (_) || |  
 \___|_|  \___/ \_/\_/ \___|_|\___/ \___/ |_|  
                        cloud                   
```

**Open-source, self-hosted cloud infrastructure — on your terms.**

[![Rust CI](https://github.com/GavinMce/crowCloud/actions/workflows/ci_rust.yml/badge.svg)](https://github.com/GavinMce/crowCloud/actions/workflows/ci_rust.yml)
[![Frontend CI](https://github.com/GavinMce/crowCloud/actions/workflows/ci_frontend.yml/badge.svg)](https://github.com/GavinMce/crowCloud/actions/workflows/ci_frontend.yml)
[![Security Audit](https://github.com/GavinMce/crowCloud/actions/workflows/ci_security.yml/badge.svg)](https://github.com/GavinMce/crowCloud/actions/workflows/ci_security.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

</div>

---

crowCloud is a self-hosted cloud management platform that lets you provision and manage virtual machines, Kubernetes clusters, object storage, and databases across multiple infrastructure providers — from your own hardware to public clouds — through a single unified API and UI.

## Overview

Most cloud management platforms are either locked to a single provider or require expensive SaaS subscriptions. crowCloud runs entirely on your own infrastructure, stores nothing externally, and treats every provider the same way through a typed provider trait.

**Key capabilities:**

- **Multi-provider VM orchestration** — Proxmox (implemented), Hetzner (stub), extensible to any bare-metal or public cloud
- **Kubernetes operator** — CRDs for VMs, networks, object stores, databases, and K8s clusters; reconciliation loops in `crow-operator`
- **REST API** — `crow-api` exposes a versioned HTTP API backed by PostgreSQL
- **CLI** — `crow` binary for scripting and terminal workflows
- **Web UI** — React + TypeScript frontend served from `frontend/`
- **Helm chart** — deploy the full platform to an existing K8s cluster
- **Automated releases** — release-plz manages changelogs and tags; Docker images and CLI binaries are published to GHCR on every tag

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                       crowCloud                         │
│                                                         │
│  ┌──────────┐   ┌─────────────┐   ┌──────────────────┐ │
│  │  crow    │   │  crow-api   │   │  crow-operator   │ │
│  │  (CLI)   │──▶│  (REST API) │   │  (K8s operator)  │ │
│  └──────────┘   └──────┬──────┘   └────────┬─────────┘ │
│                         │                   │           │
│                  ┌──────▼──────────────────▼──────┐    │
│                  │           crow-core             │    │
│                  │  (CRDs · traits · types)        │    │
│                  └────────────────────────────────┘    │
│                                                         │
│  ┌──────────────────────┐  ┌──────────────────────────┐ │
│  │   Providers          │  │   Resources              │ │
│  │  crow-provider-      │  │  crow-resource-vm        │ │
│  │    proxmox           │  │  crow-resource-k8s       │ │
│  │  crow-provider-      │  │  crow-resource-database  │ │
│  │    hetzner           │  │  crow-resource-          │ │
│  └──────────────────────┘  │    objectstore           │ │
│                            └──────────────────────────┘ │
│                                                         │
│  ┌─────────────┐  ┌──────────────┐  ┌───────────────┐  │
│  │  crow-auth  │  │   crow-db    │  │ crow-vps-agent│  │
│  │  (JWT/bcrypt│  │  (sqlx/PG)  │  │ (on-host agent│  │
│  └─────────────┘  └──────────────┘  └───────────────┘  │
└─────────────────────────────────────────────────────────┘
```

## Workspace Crates

| Crate | Purpose |
|---|---|
| `crow-core` | Shared types, CRD definitions, provider trait (`InfraProvider`), error types |
| `crow-api` | Axum REST API — projects, resource groups, providers, auth |
| `crow-operator` | Kubernetes operator — watches CRDs, reconciles resources via provider |
| `crow-auth` | JWT issuance/validation, bcrypt password hashing |
| `crow-db` | SQLx migrations and connection pool helpers |
| `crow-cli` | `crow` binary — wraps the REST API for scripting |
| `crow-vps-agent` | Lightweight Axum agent that runs on-host for VPS operations |
| `crow-provider-proxmox` | Full Proxmox VE provider: VM clone, cloud-init, bridge, storage |
| `crow-provider-hetzner` | Hetzner Cloud provider (stub) |
| `crow-resource-vm` | VM resource reconciler |
| `crow-resource-k8s` | Kubernetes cluster resource reconciler |
| `crow-resource-objectstore` | Object storage resource reconciler |
| `crow-resource-database` | Database resource reconciler |

## Getting Started

### Prerequisites

- Rust stable (≥ 1.82) — install via [rustup](https://rustup.rs)
- Node.js 20+ and npm
- PostgreSQL 15+
- A Kubernetes cluster (for the operator) or Proxmox VE (for the Proxmox provider)

### Build

```bash
# Rust workspace
cargo build --release

# Frontend
cd frontend && npm ci && npm run build
```

### Configuration

The API reads configuration from environment variables:

| Variable | Description | Default |
|---|---|---|
| `DATABASE_URL` | PostgreSQL connection string | required |
| `LISTEN_ADDR` | API bind address | `0.0.0.0:8080` |
| `JWT_SECRET` | Secret for signing JWTs | required |

The operator uses the ambient kubeconfig (in-cluster or `~/.kube/config`).

### Run locally

```bash
# Start the API
DATABASE_URL=postgres://... cargo run -p crow-api

# Start the frontend dev server
cd frontend && npm run dev

# Use the CLI
cargo run -p crow-cli -- --help
```

### Docker / Helm

Docker images are published to GHCR on every release tag:

```bash
docker pull ghcr.io/gavinmce/crow-api:latest
docker pull ghcr.io/gavinmce/crow-operator:latest
docker pull ghcr.io/gavinmce/crow-vps-agent:latest
```

Deploy with Helm:

```bash
helm upgrade --install crowcloud \
  oci://ghcr.io/gavinmce/charts/crowcloud \
  --version 0.1.0 \
  -f values.yaml
```

See [`charts/crowcloud/values.yaml`](charts/crowcloud/values.yaml) for configuration options.

## Providers

### Proxmox VE

The Proxmox provider (`crow-provider-proxmox`) implements the full `InfraProvider` trait:

- **VM lifecycle** — full clone from template, cloud-init snippet upload, disk resize, task-awaited start/stop/delete
- **Networking** — Linux bridge creation and deletion via the Proxmox network API
- **Storage** — volume intent records; actual disk allocation happens at VM creation

Configuration (passed to `ProxmoxProvider::new`):

| Field | Description |
|---|---|
| `url` | Proxmox API base URL, e.g. `https://pve.example.com:8006` |
| `token_id` | API token in `user@realm!tokenname` format |
| `token_secret` | Token secret |
| `node` | Target Proxmox node name |
| `default_storage` | Default storage ID for clones and snippets |
| `default_bridge` | Default Linux bridge, e.g. `vmbr0` |
| `tls_insecure` | Skip TLS verification (self-signed certs) |

### Adding a new provider

1. Create a new crate under `crates/providers/crow-provider-<name>/`
2. Implement `crow_core::traits::InfraProvider` for your provider struct
3. Add it to the workspace in the root `Cargo.toml`
4. Register it in `crow-operator` so the reconciler can dispatch to it

## CI / CD

| Workflow | Trigger | Jobs |
|---|---|---|
| `ci_rust.yml` | push/PR → main | Format · Clippy · Test |
| `ci_frontend.yml` | push/PR → main | Type-check · Lint · Test · Build |
| `ci_security.yml` | Weekly + `Cargo.lock`/`package-lock.json` changes | Cargo Audit · npm Audit |
| `cd_release.yml` | push → main | release-plz PR management + tag creation |
| `cd_publish.yml` | push tag `v*.*.*` | Docker images → GHCR · CLI binaries · Helm chart · GitHub Release |

Branch protection on `main` requires all four CI checks to pass before merge.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full development workflow, commit conventions, and PR process.

## License

[MIT](LICENSE)
