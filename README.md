<div align="center">

```
____________________________________________________________________/\\\\\\\\\__/\\\\\\________________________________________/\\\__
 _________________________________________________________________/\\\////////__\////\\\_______________________________________\/\\\__
  _______________________________________________________________/\\\/______________\/\\\_______________________________________\/\\\__
   _____/\\\\\\\\__/\\/\\\\\\\______/\\\\\_____/\\____/\\___/\\__/\\\________________\/\\\________/\\\\\_____/\\\____/\\\________\/\\\__
    ___/\\\//////__\/\\\/////\\\___/\\\///\\\__\/\\\__/\\\\_/\\\_\/\\\________________\/\\\______/\\\///\\\__\/\\\___\/\\\___/\\\\\\\\\__
     __/\\\_________\/\\\___\///___/\\\__\//\\\_\//\\\/\\\\\/\\\__\//\\\_______________\/\\\_____/\\\__\//\\\_\/\\\___\/\\\__/\\\////\\\__
      _\//\\\________\/\\\_________\//\\\__/\\\___\//\\\\\/\\\\\____\///\\\_____________\/\\\____\//\\\__/\\\__\/\\\___\/\\\_\/\\\__\/\\\__
       __\///\\\\\\\\_\/\\\__________\///\\\\\/_____\//\\\\//\\\_______\////\\\\\\\\\__/\\\\\\\\\__\///\\\\\/___\//\\\\\\\\\__\//\\\\\\\/\\_
        ____\////////__\///_____________\/////________\///__\///___________\/////////__\/////////_____\/////______\/////////____\///////\//__
```

**Open-source, self-hosted cloud infrastructure — on your terms.**

[![CI / Rust](https://github.com/GavinMce/crowCloud/actions/workflows/ci_rust.yml/badge.svg)](https://github.com/GavinMce/crowCloud/actions/workflows/ci_rust.yml)
[![CI / Frontend](https://github.com/GavinMce/crowCloud/actions/workflows/ci_frontend.yml/badge.svg)](https://github.com/GavinMce/crowCloud/actions/workflows/ci_frontend.yml)
[![CI / Security](https://github.com/GavinMce/crowCloud/actions/workflows/ci_security.yml/badge.svg)](https://github.com/GavinMce/crowCloud/actions/workflows/ci_security.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

</div>

---

## Table of Contents

- [What is crowCloud?](#what-is-crowcloud)
- [Features](#features)
- [Architecture](#architecture)
- [Workspace Crates](#workspace-crates)
- [Getting Started](#getting-started)
  - [Prerequisites](#prerequisites)
  - [Deploy with Helm](#deploy-with-helm)
  - [Local Development](#local-development)
- [Configuration](#configuration)
- [The Provider System](#the-provider-system)
  - [InfraProvider trait](#infraprovider-trait)
  - [NetworkProvider and DnsProvider](#networkprovider-and-dnsprovider)
  - [ResourceDriver trait](#resourcedriver-trait)
- [Providers](#providers)
  - [Proxmox VE](#proxmox-ve)
  - [Hetzner Cloud](#hetzner-cloud)
  - [Implementing a new provider](#implementing-a-new-provider)
- [Kubernetes Operator](#kubernetes-operator)
- [REST API](#rest-api)
- [CLI Reference](#cli-reference)
- [Database Schema](#database-schema)
- [CI / CD Pipeline](#ci--cd-pipeline)
- [Contributing](#contributing)
- [License](#license)

---

## What is crowCloud?

crowCloud is a self-hosted cloud management platform written in Rust and React/TypeScript. It lets you provision and manage virtual machines, Kubernetes clusters, object storage, and databases across multiple infrastructure providers — from your own bare-metal hardware to public clouds — through a single unified operator, REST API, and CLI.

Most cloud management platforms are either locked to one provider or require expensive SaaS subscriptions. crowCloud runs entirely on your own infrastructure, stores nothing externally, and treats every provider through a typed `InfraProvider` trait so switching or adding backends never requires changing application code.

---

## Features

- **Multi-provider infrastructure** — A single `InfraProvider` trait abstracts VM lifecycle, networking, and storage. Proxmox VE is fully implemented; Hetzner Cloud is a stub ready for completion. Any bare-metal or public cloud can be added as a new crate.
- **Kubernetes operator** — `crow-operator` watches custom resources (CRDs) in your cluster and reconciles them against the configured provider. Supports VMs, networks, volumes, object stores, databases, and K8s clusters.
- **REST API** — `crow-api` exposes a versioned HTTP API (`/api/v1`) backed by PostgreSQL. JWT authentication, project and resource-group scoping, full audit log.
- **CLI** — The `crow` binary covers the full workflow: login, project/resource-group context, VM management, Kubernetes, databases, object storage, expose/domain/tunnel operations, and provider registration.
- **Web UI** — React + TypeScript frontend (Vite, oxlint, vitest) served separately from the API.
- **VPS agent** — A lightweight Axum agent (`crow-vps-agent`) deployed on bare-metal hosts for tunnel and on-host operations.
- **Helm chart** — Deploy the full stack (API + operator + PostgreSQL) to an existing Kubernetes cluster with a single `helm upgrade --install`.
- **Automated releases** — release-plz manages changelogs and version tags. On every `v*.*.*` tag, Docker images are pushed to GHCR, CLI binaries are built for four targets and attached to a GitHub Release, and the Helm chart is pushed to GHCR OCI.

---

## Architecture

```
                    ┌──────────────────────────────────────────────────────────┐
                    │                     crowCloud                            │
                    │                                                          │
  ┌───────────┐     │  ┌──────────────┐          ┌──────────────────────────┐ │
  │  Browser  │────▶│  │  frontend    │          │     crow-operator        │ │
  └───────────┘     │  │  React/TS    │          │  kube-rs reconciler      │ │
                    │  └──────────────┘          │  watches CRDs → calls    │ │
  ┌───────────┐     │                            │  InfraProvider           │ │
  │  crow CLI │────▶│  ┌──────────────┐          └────────────┬─────────────┘ │
  └───────────┘     │  │  crow-api    │                       │               │
                    │  │  Axum REST   │          ┌────────────▼─────────────┐ │
                    │  │  /api/v1     │          │       crow-core          │ │
                    │  └──────┬───────┘          │  InfraProvider trait     │ │
                    │         │                  │  NetworkProvider trait   │ │
                    │  ┌──────▼───────┐          │  DnsProvider trait       │ │
                    │  │  crow-auth   │          │  ResourceDriver trait    │ │
                    │  │  JWT · bcrypt│          │  CRD structs · types     │ │
                    │  └──────────────┘          └──────┬───────────────────┘ │
                    │                                   │                     │
                    │  ┌──────────────┐     ┌──────────▼──────────┐          │
                    │  │   crow-db    │     │      Providers       │          │
                    │  │  SQLx · PG   │     │  crow-provider-      │          │
                    │  │  migrations  │     │    proxmox  ✓        │          │
                    │  └──────────────┘     │  crow-provider-      │          │
                    │                       │    hetzner  (stub)   │          │
                    │  ┌──────────────┐     └─────────────────────┘          │
                    │  │ crow-vps-    │                                       │
                    │  │ agent        │     ┌─────────────────────┐           │
                    │  │ on-host ops  │     │     Resources        │           │
                    │  └──────────────┘     │  crow-resource-vm   │           │
                    │                       │  crow-resource-k8s  │           │
                    │                       │  crow-resource-db   │           │
                    │                       │  crow-resource-      │           │
                    │                       │    objectstore      │           │
                    │                       └─────────────────────┘           │
                    └──────────────────────────────────────────────────────────┘
```

---

## Workspace Crates

The repository is a Cargo workspace. All dependency versions are pinned once in the root `Cargo.toml` under `[workspace.dependencies]`.

| Crate | Path | Role |
|---|---|---|
| `crow-core` | `crates/crow-core` | Contract layer: `InfraProvider`, `NetworkProvider`, `DnsProvider`, `ResourceDriver` traits; all shared types (`VmSpec`, `VolumeSpec`, …); CRD structs; `ProviderError` |
| `crow-api` | `crates/crow-api` | Axum 0.8 REST API on port 8080. Routes nested at `/api/v1`. State: `kube::Client` + `PgPool`. Returns `ApiError` (JSON `{ "error": "…" }`) |
| `crow-operator` | `crates/crow-operator` | kube-rs 4 operator. Watches CRDs, reconciles resources by dispatching to `InfraProvider`. Runs in-cluster or with ambient kubeconfig |
| `crow-auth` | `crates/crow-auth` | JWT issuance and validation (HS256), bcrypt password hashing |
| `crow-db` | `crates/crow-db` | SQLx connection pool, migration runner, query helpers. Migrations in `crates/crow-db/migrations/` |
| `crow-cli` | `crates/crow-cli` | `crow` binary. Thin REST client using clap subcommands. Supports `--output table\|json` and `CROW_SERVER` env override |
| `crow-vps-agent` | `crates/crow-vps-agent` | Lightweight Axum agent deployed on bare-metal VPS hosts. Handles tunnel and on-host operations. Listens on `WIREGUARD_ADDR` (default `10.200.0.1:9090`) |
| `crow-provider-proxmox` | `crates/providers/crow-provider-proxmox` | Full Proxmox VE implementation of `InfraProvider`. VM clone → cloud-init → task polling → start/stop/delete. Bridge and storage management |
| `crow-provider-hetzner` | `crates/providers/crow-provider-hetzner` | Hetzner Cloud stub — trait implemented, API calls not yet wired |
| `crow-resource-vm` | `crates/resources/crow-resource-vm` | `ResourceDriver` for bare VMs |
| `crow-resource-k8s` | `crates/resources/crow-resource-k8s` | `ResourceDriver` for managed Kubernetes clusters |
| `crow-resource-database` | `crates/resources/crow-resource-database` | `ResourceDriver` for databases |
| `crow-resource-objectstore` | `crates/resources/crow-resource-objectstore` | `ResourceDriver` for object storage buckets |

---

## Getting Started

### Prerequisites

| Requirement | Minimum version | Notes |
|---|---|---|
| Rust | stable ≥ 1.88 | Install via [rustup](https://rustup.rs) |
| Node.js | 20+ | npm included |
| PostgreSQL | 15+ | Only needed for local dev; Helm chart bundles its own |
| Kubernetes | 1.32+ | For the operator; any conformant cluster |
| Proxmox VE | 7+ | For the Proxmox provider |

### Deploy with Helm

The fastest path to a running crowCloud is the Helm chart.

```bash
# Add GHCR OCI registry (no helm repo add needed for OCI)
helm upgrade --install crowcloud \
  oci://ghcr.io/gavinmce/charts/crowcloud \
  --version 0.1.0 \
  --namespace crowcloud \
  --create-namespace \
  --set api.env.DATABASE_URL="postgres://user:pass@host:5432/crowcloud" \
  --set api.env.JWT_SECRET="$(openssl rand -hex 32)"
```

The chart deploys the API, operator, and (optionally) a bundled PostgreSQL instance. See [`charts/crowcloud/values.yaml`](charts/crowcloud/values.yaml) for all options.

**Key values:**

| Key | Default | Description |
|---|---|---|
| `api.replicas` | `1` | API pod count |
| `api.env.DATABASE_URL` | `""` | PostgreSQL connection string (required) |
| `api.env.JWT_SECRET` | `""` | JWT signing secret (required) |
| `operator.replicas` | `1` | Operator pod count |
| `postgres.enabled` | `true` | Deploy bundled PostgreSQL |
| `postgres.storageSize` | `10Gi` | PVC size for PostgreSQL data |
| `postgres.storageClass` | `""` | Storage class; empty = cluster default |
| `ingress.enabled` | `false` | Create an Ingress for the API |
| `ingress.host` | `""` | Hostname for the Ingress rule |
| `ingress.tls` | `false` | Enable TLS on the Ingress |

### Local Development

**1. Clone and set up the database**

```bash
git clone https://github.com/GavinMce/crowCloud.git
cd crowCloud

# Create the database and run migrations
createdb crowcloud
DATABASE_URL=postgres://localhost/crowcloud cargo sqlx migrate run \
  --source crates/crow-db/migrations
```

**2. Run the API**

```bash
export DATABASE_URL=postgres://localhost/crowcloud
export JWT_SECRET=dev-secret-change-in-prod
export RUST_LOG=info

cargo run -p crow-api
# API is now listening on http://localhost:8080
```

**3. Run the operator** (optional — requires a kubeconfig)

```bash
cargo run -p crow-operator
```

**4. Run the frontend dev server**

```bash
cd frontend
npm ci
npm run dev
# Frontend dev server at http://localhost:5173, proxies /api → :8080
```

**5. Use the CLI**

```bash
# Build the CLI
cargo build -p crow-cli

# Point it at your local API
export CROW_SERVER=http://localhost:8080

./target/debug/crow login
./target/debug/crow project list
```

**6. Pull pre-built Docker images**

```bash
docker pull ghcr.io/gavinmce/crow-api:latest
docker pull ghcr.io/gavinmce/crow-operator:latest
docker pull ghcr.io/gavinmce/crow-vps-agent:latest
```

**Build and test commands:**

```bash
# Check the whole workspace compiles
cargo check --all

# Lint (must be zero warnings — CI runs with -D warnings)
cargo clippy --all-targets --all-features

# Format check
cargo fmt --all -- --check

# Run all tests
cargo test --all

# Frontend checks
cd frontend && npm run type-check && npm run lint && npm run test && npm run build
```

---

## Configuration

### API (`crow-api`)

| Environment variable | Required | Default | Description |
|---|---|---|---|
| `DATABASE_URL` | yes | — | PostgreSQL connection string, e.g. `postgres://user:pass@localhost/crowcloud` |
| `JWT_SECRET` | yes | — | Secret key for signing and verifying JWTs (HS256) |
| `LISTEN_ADDR` | no | `0.0.0.0:8080` | TCP address and port for the API to bind |
| `RUST_LOG` | no | `info` | Log filter, e.g. `crow_api=debug,info` |

### Operator (`crow-operator`)

The operator uses the ambient kubeconfig: in-cluster (`KUBERNETES_SERVICE_HOST`) or `~/.kube/config` for local development. No additional environment variables are required.

### VPS Agent (`crow-vps-agent`)

| Environment variable | Required | Default | Description |
|---|---|---|---|
| `WIREGUARD_ADDR` | no | `10.200.0.1:9090` | Address the agent binds to on the VPS host |

---

## The Provider System

crowCloud's extensibility is built on three async traits defined in `crow-core`. Every provider crate implements one or more of them. The operator dispatches to providers via `Arc<dyn InfraProvider>` — no enum matching, no provider-specific code outside the provider crate.

### InfraProvider trait

```rust
#[async_trait]
pub trait InfraProvider: Send + Sync {
    fn provider_type(&self) -> &'static str;
    fn name(&self) -> &str;

    // VM lifecycle
    async fn create_vm(&self, spec: VmSpec) -> Result<VmHandle, ProviderError>;
    async fn delete_vm(&self, handle: &VmHandle) -> Result<(), ProviderError>;
    async fn vm_status(&self, handle: &VmHandle) -> Result<VmStatus, ProviderError>;
    async fn start_vm(&self, handle: &VmHandle) -> Result<(), ProviderError>;
    async fn stop_vm(&self, handle: &VmHandle) -> Result<(), ProviderError>;

    // Storage
    async fn create_volume(&self, spec: VolumeSpec) -> Result<VolumeHandle, ProviderError>;
    async fn delete_volume(&self, handle: &VolumeHandle) -> Result<(), ProviderError>;

    // Networking
    async fn create_network(&self, spec: NetworkSpec) -> Result<NetworkHandle, ProviderError>;
    async fn delete_network(&self, handle: &NetworkHandle) -> Result<(), ProviderError>;
}
```

**Key types:**

| Type | Fields |
|---|---|
| `VmSpec` | `name`, `cpu`, `memory_mib`, `disk_gib`, `image`, `ip?`, `cloud_init?`, `network_ref?` |
| `VmHandle` | `provider_type`, `provider_id`, `ip?`, `name` — opaque reference returned after creation |
| `VmStatus` | `Running` / `Stopped` / `Starting` / `Stopping` / `Error(String)` / `Unknown` |
| `VolumeSpec` | `name`, `size_gib`, `storage_pool?` |
| `NetworkSpec` | `name`, `cidr?`, `vlan_id?`, `bridge?` |
| `CloudInitConfig` | `hostname`, `user_data?`, `network_config?` |

Error mapping: provider-specific errors (e.g. `ProxmoxError`) implement `From<ProxmoxError> for ProviderError` and compose with `?`.

### NetworkProvider and DnsProvider

```rust
#[async_trait]
pub trait NetworkProvider: Send + Sync {
    async fn expose_http(&self, spec: HttpExposeSpec) -> Result<ExposeHandle, ProviderError>;
    async fn expose_tcp(&self, spec: TcpExposeSpec) -> Result<ExposeHandle, ProviderError>;
    async fn unexpose(&self, handle: &ExposeHandle) -> Result<(), ProviderError>;
    async fn provision_cert(&self, domain: &str) -> Result<CertHandle, ProviderError>;
    async fn revoke_cert(&self, handle: &CertHandle) -> Result<(), ProviderError>;
}

#[async_trait]
pub trait DnsProvider: Send + Sync {
    async fn create_record(&self, spec: DnsRecordSpec) -> Result<DnsRecordHandle, ProviderError>;
    async fn delete_record(&self, handle: &DnsRecordHandle) -> Result<(), ProviderError>;
    async fn update_record(&self, handle: &DnsRecordHandle, spec: DnsRecordSpec) -> Result<(), ProviderError>;
}
```

`DnsRecordSpec` supports A, AAAA, CNAME, TXT, and MX record types.

### ResourceDriver trait

Resource drivers sit above providers. They orchestrate multi-step provisioning (infra → bootstrap → health-check) for higher-level resources like managed Kubernetes clusters or databases.

```rust
#[async_trait]
pub trait ResourceDriver: Send + Sync {
    fn resource_type(&self) -> &'static str;
    fn config_schema(&self) -> Value;

    async fn provision(&self, ctx: &ProvisionCtx) -> Result<ResourceHandle, DriverError>;
    async fn deprovision(&self, ctx: &ProvisionCtx, handle: &ResourceHandle) -> Result<(), DriverError>;
    async fn reconcile(&self, ctx: &ProvisionCtx, handle: &ResourceHandle) -> Result<ResourcePhase, DriverError>;
    async fn endpoints(&self, handle: &ResourceHandle) -> Result<Vec<Endpoint>, DriverError>;
    async fn credentials(&self, handle: &ResourceHandle) -> Result<Value, DriverError>;
}
```

`ProvisionCtx` carries `Arc<dyn InfraProvider>`, optional `Arc<dyn NetworkProvider>` and `Arc<dyn DnsProvider>`, and the project/resource-group/resource-name scope.

**ResourcePhase lifecycle:**

```
Pending → ProvisioningInfra → Bootstrapping → HealthChecking → Ready
                                                                  │
                                          Degraded ←─────────────┤
                                          Scaling  ←─────────────┤
                                          Upgrading ←────────────┤
                                          Deleting → Deleted
                                          Failed
```

---

## Providers

### Proxmox VE

`crow-provider-proxmox` is the reference provider and the only fully implemented one. It implements `InfraProvider` against the Proxmox VE REST API.

**What it does:**

- **VM creation** — Clones a template VM by VMID, resizes the root disk, uploads a cloud-init snippet to Proxmox storage (multipart), sets the cloud-init drive, then starts the VM. Each async Proxmox call returns a UPID (task ID) which is polled every 2 s until the task settles.
- **VM deletion** — Stops the VM if running (task-awaited), then deletes it.
- **VM status** — Maps Proxmox `qmpstatus` → `VmStatus`.
- **Networking** — Creates and deletes Linux bridges via the Proxmox network API.
- **Storage** — Volume intent records; disk allocation happens at VM creation time.

**Configuration** (passed to `ProxmoxProvider::new`):

| Field | Type | Description |
|---|---|---|
| `url` | `String` | Proxmox API base URL, e.g. `https://pve.example.com:8006` |
| `token_id` | `String` | API token in `user@realm!tokenname` format |
| `token_secret` | `String` | Token secret UUID |
| `node` | `String` | Target Proxmox node name, e.g. `pve` |
| `default_storage` | `String` | Storage ID for clones and cloud-init snippets, e.g. `local-lvm` |
| `default_bridge` | `String` | Default Linux bridge, e.g. `vmbr0` |
| `tls_insecure` | `bool` | Skip TLS certificate verification (needed for self-signed certs) |

**Known limitations:**

- Cloud-init snippet files are uploaded to Proxmox storage before VM creation. If VM creation fails after the upload, the snippet is orphaned — there is currently no cleanup path.
- `wait_task` polls every 2 s with no timeout; a stuck Proxmox task will block indefinitely.

### Hetzner Cloud

`crow-provider-hetzner` contains the trait implementation skeleton. The `InfraProvider` methods are stubbed and return `ProviderError::NotSupported`. Contributions welcome — see [CONTRIBUTING.md](CONTRIBUTING.md).

### Implementing a new provider

1. Create a new crate: `crates/providers/crow-provider-<name>/`

2. Add it to the workspace `Cargo.toml`:
   ```toml
   [workspace]
   members = [
     # ...
     "crates/providers/crow-provider-<name>",
   ]
   ```

3. Implement `InfraProvider` (and optionally `NetworkProvider`, `DnsProvider`):
   ```rust
   use crow_core::{traits::InfraProvider, types::*, error::ProviderError};
   use async_trait::async_trait;

   pub struct MyProvider { /* config fields */ }

   #[async_trait]
   impl InfraProvider for MyProvider {
       fn provider_type(&self) -> &'static str { "my-provider" }
       fn name(&self) -> &str { &self.name }

       async fn create_vm(&self, spec: VmSpec) -> Result<VmHandle, ProviderError> {
           // call your API here
       }
       // ... implement remaining methods
   }
   ```

4. Map provider-specific errors:
   ```rust
   impl From<MyProviderError> for ProviderError {
       fn from(e: MyProviderError) -> Self {
           ProviderError::Provider(e.to_string())
       }
   }
   ```

5. Register the provider in `crow-operator` so the reconciler can dispatch to it.

---

## Kubernetes Operator

`crow-operator` uses [kube-rs](https://github.com/kube-rs/kube) 4.x to watch custom resources and reconcile them against the configured provider.

**CRDs** are defined in `crow-core/src/crd/` using `#[derive(CustomResource, JsonSchema)]` from kube-derive and schemars 1.x. The workspace targets Kubernetes 1.32 (`k8s-openapi` feature `v1_32`).

> **Note for library crate authors:** Do not enable a `v1_*` feature on library crates — only the binary crate (`crow-operator`) enables this via the workspace `Cargo.toml`. Enabling it in a library creates a Cargo feature conflict.

The operator runs in-cluster (using the pod's service account) or locally against `~/.kube/config`. It requires RBAC permissions to read/write the CRDs it manages and to create/patch resources in the target namespace.

---

## REST API

The API is served at `http://<host>:8080/api/v1`. All responses are JSON. Errors return `{ "error": "<message>" }` with an appropriate HTTP status.

Authentication uses JWT bearer tokens. Obtain a token via `POST /api/v1/auth/login`. Pass it as `Authorization: Bearer <token>` on subsequent requests.

**Route modules** (implementation in progress):

| Prefix | Module | Description |
|---|---|---|
| `/api/v1/auth` | `routes::auth` | Login, logout, token refresh |
| `/api/v1/projects` | `routes::projects` | Project CRUD |
| `/api/v1/rg` | `routes::resource_groups` | Resource group management |
| `/api/v1/resources` | `routes::resources` | Resource provisioning and status |
| `/api/v1/expose` | `routes::expose` | HTTP/TCP expose management |

**Axum patterns:** Routes return `Result<impl IntoResponse, ApiError>`. `ApiError` implements `IntoResponse` and maps to JSON error responses. State is `AppState` (`kube::Client` + `PgPool`), threaded through `Router::with_state`.

---

## CLI Reference

The `crow` binary is built from `crow-cli`. Install it from a [GitHub Release](https://github.com/GavinMce/crowCloud/releases) or build from source:

```bash
cargo install --path crates/crow-cli
```

**Global flags:**

| Flag | Env | Default | Description |
|---|---|---|---|
| `-o, --output` | — | `table` | Output format: `table` or `json` |
| `--server` | `CROW_SERVER` | — | crowCloud API URL (overrides saved config) |

**Subcommands:**

| Command | Description |
|---|---|
| `crow login` | Authenticate and save a session token |
| `crow context` | Set or display the active project and resource group |
| `crow project` | Create, list, get, and delete projects |
| `crow rg` | Manage resource groups within a project |
| `crow vm` | Provision, list, start, stop, and delete virtual machines |
| `crow k8s` | Provision and manage Kubernetes clusters |
| `crow db` | Provision and manage databases |
| `crow store` | Manage object storage buckets |
| `crow expose` | Expose services over HTTP or TCP |
| `crow domain` | Manage custom domains and TLS certificates |
| `crow provider` | Register and list infrastructure providers |
| `crow tunnel` | Manage the VPS agent tunnel endpoint |

Use `crow <subcommand> --help` for flags and subcommand details.

---

## Database Schema

Migrations live in `crates/crow-db/migrations/` and are run automatically on startup.

**`users`**

| Column | Type | Notes |
|---|---|---|
| `id` | `UUID` | PK, auto-generated |
| `username` | `VARCHAR(255)` | Unique |
| `email` | `VARCHAR(255)` | Unique |
| `password_hash` | `VARCHAR(255)` | bcrypt |
| `is_admin` | `BOOLEAN` | Default false |
| `created_at` / `updated_at` | `TIMESTAMPTZ` | Auto-set |

**`sessions`**

| Column | Type | Notes |
|---|---|---|
| `id` | `UUID` | PK |
| `user_id` | `UUID` | FK → `users.id` |
| `token_hash` | `VARCHAR(255)` | Unique, SHA-256 of the JWT |
| `expires_at` | `TIMESTAMPTZ` | Checked on every request |

Indexed on `user_id` and `expires_at`.

**`audit_log`**

Every mutating API action is appended here. Never updated or deleted.

| Column | Type | Notes |
|---|---|---|
| `id` | `UUID` | PK |
| `user_id` | `UUID` | FK → `users.id`, nullable for system actions |
| `action` | `VARCHAR(255)` | e.g. `vm.create`, `project.delete` |
| `resource_kind` | `VARCHAR(255)` | e.g. `VirtualMachine` |
| `resource_name` | `VARCHAR(255)` | |
| `project` | `VARCHAR(255)` | |
| `namespace` | `VARCHAR(255)` | |
| `details` | `JSONB` | Full request/response payload |
| `created_at` | `TIMESTAMPTZ` | Indexed DESC |

---

## CI / CD Pipeline

| Workflow | File | Trigger | Required check |
|---|---|---|---|
| CI / Rust | `ci_rust.yml` | push / PR → `main` | `Format`, `Clippy`, `Test` |
| CI / Frontend | `ci_frontend.yml` | push / PR → `main` | `Check` |
| CI / Security | `ci_security.yml` | Weekly Monday 08:00 UTC + `Cargo.lock` / `package-lock.json` changes | — |
| CD / Release | `cd_release.yml` | push → `main` | — |
| CD / Publish | `cd_publish.yml` | push tag `v*.*.*` | — |

Branch protection on `main` enforces all four required checks and `enforce_admins` — even the repository owner must go through a PR. Direct pushes are blocked.

**Release flow:**

1. Conventional commits (`feat:`, `fix:`, breaking `!`) trigger release-plz to open or update a "Release v*X.Y.Z*" PR.
2. Merging that PR causes `cd_release.yml` to create a `v*X.Y.Z*` git tag (via the `crow-api` package, configured as the workspace release driver in `release-plz.toml`).
3. The tag push triggers `cd_publish.yml`, which:
   - Builds Docker images for `crow-api`, `crow-operator`, `crow-vps-agent` (using `lukemathwalker/cargo-chef:latest-rust-1` for layer caching) and pushes to GHCR.
   - Cross-compiles the `crow` CLI for four targets: `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `x86_64-apple-darwin`, `aarch64-apple-darwin`.
   - Packages and pushes the Helm chart to `oci://ghcr.io/gavinmce/charts`.
   - Creates a GitHub Release with the CLI binary archives attached.

> **RELEASE_PLZ_TOKEN:** The `cd_release.yml` workflow uses `RELEASE_PLZ_TOKEN` (a PAT with `repo` scope) for tag pushes. Without it the workflow falls back to `GITHUB_TOKEN`, and tags pushed by `GITHUB_TOKEN` will not trigger downstream workflows like `cd_publish.yml`.

**Rust CI flags:**

```
RUSTFLAGS="-D warnings"
```

All crates compile with warnings-as-errors. Stubs and placeholder structs that intentionally have dead code use `#[allow(dead_code)]`.

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for the full development workflow, branch naming, conventional commit guide, PR process, and instructions for adding a new provider.

---

## License

[MIT](LICENSE)
