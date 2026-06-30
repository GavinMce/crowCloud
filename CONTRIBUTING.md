# Contributing to crowCloud

Thank you for your interest in contributing. This document covers how to get set up, the conventions we follow, and the process for getting changes merged.

## Table of Contents

- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Making Changes](#making-changes)
- [Commit Conventions](#commit-conventions)
- [Pull Request Process](#pull-request-process)
- [Adding a Provider](#adding-a-provider)
- [Code Style](#code-style)

## Development Setup

### Prerequisites

| Tool | Version | Install |
|---|---|---|
| Rust | stable ≥ 1.82 | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` |
| Node.js | 20+ | [nodejs.org](https://nodejs.org) or `nvm` |
| PostgreSQL | 15+ | `brew install postgresql` / distro package |
| kubectl | any | [install guide](https://kubernetes.io/docs/tasks/tools/) |

### Clone and build

```bash
git clone https://github.com/GavinMce/crowCloud.git
cd crowCloud

# Rust crates
cargo build

# Frontend
cd frontend && npm ci
```

### Run the checks locally

These are the same checks that CI runs — run them before pushing:

```bash
# Rust
cargo fmt --all -- --check
cargo clippy --all-targets --all-features
cargo test --all

# Frontend
cd frontend
npm run type-check
npm run lint
npm run test
npm run build
```

### Security audit

```bash
cargo audit
cd frontend && npm audit --audit-level=high
```

## Project Structure

```
crowCloud/
├── crates/
│   ├── crow-core/          # Shared types, CRDs, InfraProvider trait
│   ├── crow-api/           # Axum REST API
│   ├── crow-auth/          # JWT + password hashing
│   ├── crow-db/            # SQLx pool + migrations
│   ├── crow-operator/      # Kubernetes operator
│   ├── crow-cli/           # `crow` CLI binary
│   ├── crow-vps-agent/     # On-host agent
│   ├── providers/
│   │   ├── crow-provider-proxmox/
│   │   └── crow-provider-hetzner/
│   └── resources/
│       ├── crow-resource-vm/
│       ├── crow-resource-k8s/
│       ├── crow-resource-database/
│       └── crow-resource-objectstore/
├── frontend/               # React + TypeScript UI
├── charts/crowcloud/       # Helm chart
├── docker/                 # Dockerfiles for each service binary
├── deploy/                 # Bootstrap scripts
├── conversations/          # Saved Claude Code session context
└── .github/
    ├── workflows/          # CI/CD workflows
    ├── ISSUE_TEMPLATE/
    └── PULL_REQUEST_TEMPLATE.md
```

## Making Changes

1. **Open or claim an issue** — check existing issues before starting. For significant changes, open an issue first to discuss the approach.

2. **Branch from `main`** — use a descriptive branch name following the pattern below:

   | Type | Pattern | Example |
   |---|---|---|
   | Feature | `feat/<short-description>` | `feat/hetzner-provider` |
   | Bug fix | `fix/<short-description>` | `fix/proxmox-stop-timeout` |
   | Refactor | `refactor/<short-description>` | `refactor/operator-reconciler` |
   | Docs | `docs/<short-description>` | `docs/provider-guide` |
   | CI/tooling | `ci/<short-description>` | `ci/cargo-cache` |

3. **Keep changes focused** — one logical change per PR. Split unrelated fixes into separate PRs.

4. **Update tests** — if your change affects observable behaviour, add or update tests.

## Commit Conventions

We use [Conventional Commits](https://www.conventionalcommits.org/) because release-plz reads them to generate changelogs and determine version bumps automatically.

```
<type>(<scope>): <short summary>

[optional body]

[optional footer]
```

### Types

| Type | When to use | Version bump |
|---|---|---|
| `feat` | New user-facing feature | minor |
| `fix` | Bug fix | patch |
| `refactor` | Code restructuring, no behaviour change | none |
| `perf` | Performance improvement | patch |
| `test` | Adding or fixing tests | none |
| `docs` | Documentation only | none |
| `ci` | CI/CD workflow changes | none |
| `chore` | Maintenance (deps, tooling) | none |
| `build` | Build system changes | none |

Append `!` to any type for a breaking change, which triggers a major bump:

```
feat(api)!: remove deprecated /v0 endpoints
```

### Scope (optional but encouraged)

Use the crate short name as scope: `api`, `operator`, `cli`, `core`, `proxmox`, `hetzner`, `auth`, `db`, `frontend`.

### Examples

```
feat(proxmox): add IPv6 support to build_ipconfig
fix(operator): await stop UPID before issuing delete
ci: add rust-cache to test job
chore(deps): bump axum from 0.8.9 to 0.8.10
docs: add provider implementation guide to CONTRIBUTING
```

## Pull Request Process

1. **Open the PR** against `main`. Fill in the PR template fully.

2. **CI must be green** — all four required checks must pass:
   - `Rust CI / Format`
   - `Rust CI / Clippy`
   - `Rust CI / Test`
   - `Frontend CI / Check`

3. **Self-review your diff** before requesting review. Look for:
   - Dead code or unused imports introduced
   - Unwraps that should be handled
   - Hardcoded values that should be configurable
   - Missing error propagation

4. **One approval required** for merge to `main`.

5. **Squash or rebase** — keep `main`'s history linear. Merge commits are not used.

## Adding a Provider

Providers implement `crow_core::traits::InfraProvider`. Here is the minimal skeleton:

```rust
// crates/providers/crow-provider-<name>/src/lib.rs
use async_trait::async_trait;
use crow_core::{traits::InfraProvider, types::*, ProviderError};

pub struct MyProvider { /* config */ }

#[async_trait]
impl InfraProvider for MyProvider {
    fn provider_type(&self) -> &'static str { "my-provider" }
    fn name(&self) -> &str { &self.name }

    async fn create_vm(&self, spec: VmSpec) -> Result<VmHandle, ProviderError> { todo!() }
    async fn delete_vm(&self, handle: &VmHandle) -> Result<(), ProviderError> { todo!() }
    async fn vm_status(&self, handle: &VmHandle) -> Result<VmStatus, ProviderError> { todo!() }
    async fn start_vm(&self, handle: &VmHandle) -> Result<(), ProviderError> { todo!() }
    async fn stop_vm(&self, handle: &VmHandle) -> Result<(), ProviderError> { todo!() }

    async fn create_volume(&self, spec: VolumeSpec) -> Result<VolumeHandle, ProviderError> { todo!() }
    async fn delete_volume(&self, handle: &VolumeHandle) -> Result<(), ProviderError> { todo!() }

    async fn create_network(&self, spec: NetworkSpec) -> Result<NetworkHandle, ProviderError> { todo!() }
    async fn delete_network(&self, handle: &NetworkHandle) -> Result<(), ProviderError> { todo!() }
}
```

Use the Proxmox provider (`crates/providers/crow-provider-proxmox/`) as a reference implementation — it covers the full task-polling pattern, cloud-init handling, and error mapping to `ProviderError`.

## Code Style

- **No `unwrap()` or `expect()`** in library code. Use `?` or return a typed error.
- **No `todo!()` in production paths** — use it only in stubs that are clearly not yet implemented.
- **Comments only for non-obvious WHY**, not what: the code explains what, comments explain why a non-obvious decision was made.
- **`-D warnings` is always on** — code must compile with zero warnings.
- **RUSTFLAGS="-D warnings"** is set in CI; run `cargo clippy --all-targets` locally before pushing.
- Prefer `tracing::{info, warn, error}` over `println!` for any observable output in binary crates.
