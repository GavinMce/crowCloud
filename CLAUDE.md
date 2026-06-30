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
│   │   └── crow-provider-hetzner/   # Hetzner Cloud stub
│   └── resources/
│       ├── crow-resource-vm/
│       ├── crow-resource-k8s/
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

`crow-core/src/traits.rs` defines `InfraProvider` — the central abstraction. All providers implement this async trait. See `crow-provider-proxmox` for a complete implementation with task polling, cloud-init snippet upload, and error mapping.

Error mapping: provider-specific errors (e.g. `ProxmoxError`) implement `From<ProxmoxError> for ProviderError` so they compose with `?`.

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
| `Rust CI / Format` | `ci_rust.yml` |
| `Rust CI / Clippy` | `ci_rust.yml` |
| `Rust CI / Test` | `ci_rust.yml` |
| `Frontend CI / Check` | `ci_frontend.yml` |

The workflow `name:` fields must not change — branch protection is keyed to "Workflow name / Job name".

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
- **Cloud-init snippets** — uploaded via multipart to Proxmox storage. If VM creation fails after snippet upload, the snippet is orphaned (known limitation — no cleanup path yet).
- **axum 0.8** — `axum::serve` API is stable. Do not use deprecated `axum::Server` from 0.7.
