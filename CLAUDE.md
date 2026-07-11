# crowCloud — Claude Code Reference

This file gives Claude Code (and any AI assistant) the context needed to navigate and work in this repository effectively.

## What this project is

crowCloud is a self-hosted cloud infrastructure management platform written in Rust (workspace) + React/TypeScript (frontend). It lets users provision and manage VMs, Kubernetes clusters, object storage, and databases across multiple providers (Proxmox, Hetzner, …) through a unified operator, REST API, and CLI.

## Repository layout

```
crowCloud/
├── Cargo.toml                  # Workspace root — all [workspace.dependencies] live here
├── crates/
│   ├── crow-core/              # The contract layer: CRD structs, InfraProvider trait, shared error types
│   ├── crow-api/               # Axum REST API (port 8080)
│   ├── crow-auth/              # JWT issuance/validation, bcrypt password hashing
│   ├── crow-db/                # SQLx Postgres pool + migrations (crates/crow-db/migrations/)
│   ├── crow-operator/          # kube-rs operator — watches CRDs, reconciles via InfraProvider
│   ├── crow-cli/               # `crow` binary — thin REST client wrapper
│   ├── crow-vps-agent/         # Lightweight Axum agent deployed on VPS hosts
│   ├── providers/
│   │   ├── crow-provider-proxmox/   # Full Proxmox VE implementation (reference provider)
│   │   ├── crow-provider-opnsense/  # IpamProvider — Kea DHCPv4 reservations for static VM IPs
│   │   ├── crow-provider-hetzner/   # Hetzner Cloud stub
│   │   └── crow-provider-registry/  # Sync factories: build_infra_provider / build_ipam_provider
│   └── resources/
│       ├── crow-resource-vm/
│       ├── crow-resource-k8s/       # HA k3s on Proxmox + OPNsense — fully implemented
│       ├── crow-resource-database/
│       └── crow-resource-objectstore/
├── frontend/                   # React + TypeScript (Vite, oxlint, vitest)
├── charts/crowcloud/           # Helm chart
├── docker/                     # Dockerfile.api / .operator / .vps-agent (cargo-chef pattern)
├── deploy/                     # bootstrap.sh
├── conversations/              # Saved Claude Code session transcripts
└── .github/
    ├── workflows/
    │   ├── ci_rust.yml         # Format · Clippy · Test
    │   ├── ci_frontend.yml     # Type-check · Lint · Test · Build
    │   ├── ci_security.yml     # cargo audit + npm audit (weekly + lock-file changes)
    │   ├── cd_release.yml      # release-plz: manages release PRs and tags on push to main
    │   └── cd_publish.yml      # On tag v*.*.*: Docker → GHCR, CLI binaries, Helm, GitHub Release
    ├── ISSUE_TEMPLATE/
    └── PULL_REQUEST_TEMPLATE.md
```

## Build and test commands

```bash
# Check the whole workspace compiles
cargo check --all

# Lint
cargo clippy --all-targets --all-features   # must be zero warnings (-D warnings in CI)

# Format check
cargo fmt --all -- --check

# Tests
cargo test --all

# Frontend
cd frontend
npm ci
npm run type-check
npm run lint
npm run test
npm run build
```

RUSTFLAGS="-D warnings" is set in all CI jobs. Always run clippy before committing Rust code.

## Key patterns and conventions

### Provider trait

`crow-core/src/traits.rs` defines `InfraProvider` — the central abstraction. All providers implement this async trait. See `crow-provider-proxmox` for a complete implementation with task polling, cloud-init, guest command execution, and error mapping. `IpamProvider` is a separate trait for static IP allocation (`crow-provider-opnsense` is the only implementation), resolved independently via `ProvisionCtx.ipam`.

Error mapping: provider-specific errors (e.g. `ProxmoxError`) implement `From<ProxmoxError> for ProviderError` so they compose with `?`.

### Guest command execution — SSH, not the QEMU guest agent

`InfraProvider::exec_in_vm` runs a command inside a VM. In `crow-provider-proxmox` this goes over **SSH**, not the QEMU guest agent — the `"agent":1` VM config flag only tells Proxmox to listen for the agent, it does not guarantee `qemu-guest-agent` is installed/running inside the guest OS, and it usually isn't in a stock cloud image. Every VM gets a provider-wide Ed25519 keypair injected via Proxmox's native `sshkeys`/`ciuser` cloud-init fields (`crow_provider_proxmox::ssh::generate_keypair`); the same keypair authenticates back in. `ProxmoxConfig.ssh_private_key`/`ssh_public_key` must be generated once per provider and stored in its config — there is no in-code auto-generation at registration time.

### CRDs and schemars

CRD structs in `crow-core/src/crd/` use `#[derive(CustomResource, JsonSchema)]` (kube-derive + schemars). The workspace uses `k8s-openapi` with feature `v1_32` (Kubernetes 1.32). Do NOT enable a `v1_*` feature on library crates — only the binary crates (`crow-operator`) do this via the workspace Cargo.toml.

### Axum patterns

The API uses `axum 0.8`. Routes return `Result<impl IntoResponse, ApiError>`. `ApiError` implements `IntoResponse` and maps to JSON `{ "error": "..." }` responses. State is `AppState` (kube Client + PgPool), threaded through `Router::with_state`.

### Workspace dependencies

All dependency versions live in `[workspace.dependencies]` in the root `Cargo.toml`. Crates reference them with `dep.workspace = true`. Never pin a version differently in an individual crate's `Cargo.toml` unless there is a concrete reason.

### Commit style

Conventional Commits are required — release-plz reads them to generate changelogs. Format: `type(scope): description`. Common types: `feat`, `fix`, `refactor`, `ci`, `chore`, `docs`. Use `!` suffix for breaking changes.

## CI / CD

### Required checks (branch protection on `main`)

| Check name | Workflow |
|---|---|
| `Format` | `ci_rust.yml` |
| `Clippy` | `ci_rust.yml` |
| `Test` | `ci_rust.yml` |
| `Check` | `ci_frontend.yml` |

Branch protection uses bare job names, not "Workflow name / Job name" — so the workflow `name:` fields can be changed freely without breaking protection rules.

### Release flow

1. Every push to `main` — `cd_release.yml` runs release-plz, which creates/updates a "Release crowCloud vX.Y.Z" PR if there are unreleased conventional commits.
2. Merging that PR → release-plz creates the git tag.
3. Tag push → `cd_publish.yml` builds Docker images (→ GHCR), CLI binaries (4 targets), Helm chart (→ GHCR OCI), and a GitHub Release.

`RELEASE_PLZ_TOKEN` secret: a GitHub OAuth token with `repo` scope. Without it the workflow falls back to `GITHUB_TOKEN`, but tags created by `GITHUB_TOKEN` won't trigger `cd_publish.yml`.

### Security audit

`ci_security.yml` uses `rustsec/audit-check@v2` which needs `checks: write` permission (already set). The RSA advisory RUSTSEC-2023-0071 is currently unpatched — it is ignored in `.cargo/audit.toml`.

## Things to watch out for

- **k8s-openapi and kube must be bumped together** — kube 4.x requires k8s-openapi 0.28+. Dependabot groups them but may miss updating `k8s-openapi` when bumping `kube`.
- **Dead code lint** — all crates compile with `-D warnings`. Stubs and placeholder structs use `#[allow(dead_code)]` explicitly.
- **Proxmox task polling** — `ProxmoxClient::wait_task` polls UPID status every 2 s until stopped. `exitstatus = None` means "not yet settled — keep polling" (not success). Always await the returned UPID from async Proxmox API calls.
- **`create_vm` cleans up on ANY post-clone failure** — snippet upload, config-apply, resize, start, or the SSH bootstrap script can all fail after the clone succeeds; every path falls through to `delete_vm` so a failed/retried reconcile never leaks a VM. Preserve this if you touch `crow-provider-proxmox/src/vm.rs`.
- **Proxmox VE 9.2's `/storage/{storage}/upload` API does not accept `content=snippets`** — confirmed live; the endpoint's `content` enum is hard-limited to `iso, vztmpl, import` regardless of the storage's own configured content types. Cloud-init custom `user_data`/`network_config` snippets cannot be delivered this way on this Proxmox version. `crow-resource-k8s` works around it by booting with only native cloud-init (`ipconfig0`/`ciuser`/`sshkeys`) and running the k3s bootstrap script post-boot over SSH instead.
- **Operator status patches must be conditional** — `kube_runtime::Controller`'s default watcher re-triggers reconciliation on *any* change to the watched object, including its own `.status` patches. Unconditionally patching `.status` every reconcile causes a self-sustaining reconcile storm (every 1-2s) that can starve a real workload's CPU (this visibly destabilized a k3s node's etcd during testing). Both `virtual_machine.rs` and `k8s_cluster.rs` now compare the computed status against the current one and only patch when it actually differs, and only bump `last_transition_time` on a genuine transition.
- **axum 0.8** — `axum::serve` API is stable. Do not use deprecated `axum::Server` from 0.7.
