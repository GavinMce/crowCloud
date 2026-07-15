# Contributing to crowCloud

Thank you for your interest in contributing. This document covers how to get set up, the conventions we follow, and the process for getting changes merged.

## Table of Contents

- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Making Changes](#making-changes)
- [Commit Conventions](#commit-conventions)
- [Pull Request Process](#pull-request-process)
- [Adding a Provider](#adding-a-provider)
- [Frontend Development](#frontend-development)
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

## Frontend Development

The frontend (`frontend/`) is React + TypeScript on Vite, styled with an
internal design-system kit rather than a third-party component library. Its
information architecture is deliberately modeled on the Azure Portal: a
left icon rail of **service hubs** (Compute, Containers, Storage, Databases,
Networking, Infrastructure), each with its own Overview / All resources /
per-resource-type tabs, and resource detail pages (a specific VM, Proxmox
host, etc.) sharing the exact same page shell as the hubs themselves.

### Directory structure

```
frontend/src/
├── api/         # TanStack Query hooks + fetch wrapper, one file per backend
│                  resource (auth.ts, client.ts, projects.ts, providers.ts,
│                  resources.ts) — thin, no UI concerns
├── auth/        # useAuth, RequireAuth route guard
├── hooks/       # app-wide hooks not tied to one API resource
│                  (useCurrentProject / ProjectProvider)
├── layout/      # app-wide chrome: AppShell, TopBar, GlobalNav, HubLayout,
│                  EntityLayout — see below
├── ui/          # internal design-system primitives (Button, TextField,
│                  Select, DataTable, Modal, Tabs, Breadcrumb, CommandBar,
│                  EssentialsGrid, StatusPill, ServiceTile, icons.tsx) +
│                  theme.css
├── hubs/        # one directory per hub with a live resource type
│                  (compute/, infrastructure/) plus hub-shared scaffolding:
│   ├── hubConfig.ts                    # single source of truth: every
│   │                                     hub's id/label/icon/resource types
│   ├── HubOverviewPage.tsx             # generic Overview, for hubs whose
│   │                                     resource types are `resources`-
│   │                                     table-backed (reads apiResourceType)
│   ├── AllResourcesPage.tsx            # generic All-resources list, same
│   │                                     data assumption as above
│   ├── PlaceholderResourceTypePage.tsx # generic "not available yet" tab
│   ├── compute/, infrastructure/...    # bespoke pages for a hub's live
│   │                                     resource type(s) — list, create,
│   │                                     detail
│   └── management/                     # Projects — not a resource-type
│                                          hub, just flat CRUD pages
├── pages/       # pages outside the hub system (HomePage, LoginPage)
└── App.tsx      # the route tree
```

### The EntityLayout pattern

A service hub and a specific resource are the same *kind* of page — a
named entity reachable via breadcrumb, with a type, a side-menu of views,
and content — so both render through the one shared
`src/layout/EntityLayout.tsx` shell instead of each hand-rolling their own
header/nav:

```tsx
<EntityLayout
  breadcrumb={[{ label: 'Infrastructure', to: '/infrastructure' }, { label: host.name }]}
  type="Proxmox Host"      // small eyebrow above the name
  name={host.name}         // the H1
  navItems={NAV_ITEMS}     // this entity's own side-menu
>
  <Outlet context={host} />
</EntityLayout>
```

- **`HubLayout`** wraps `EntityLayout` for the 6 service hubs, deriving its
  nav (Overview / All resources / each resource type) from `hubConfig.ts`.
- Every resource type with a real detail page (Proxmox Host, Proxmox Node,
  Virtual Machine) has its own thin `<Thing>Layout.tsx` that fetches its
  data and wraps `EntityLayout` the same way — see
  `src/hubs/infrastructure/host/ProxmoxHostLayout.tsx` as the reference.
- **One side-nav at a time, never stacked.** This is a routing rule, not
  just a visual one: in `App.tsx`, only list-browsing routes (a hub's
  Overview/All resources/a resource type's *list*) stay nested inside
  `HubLayout`'s `<Outlet/>`. Every Create route and every resource-detail
  route is a **top-level sibling** of its hub, so navigating into one
  un-mounts the hub's nav entirely instead of piling a second nav on top of
  it.
- **Breadcrumbs are the entity chain only** — Home → hub → resource name →
  sub-resource name. Never include a side-nav tab's own label (neither a
  hub's resource-type tab like "Proxmox hosts", nor a resource's own tab
  like "Nodes") as a breadcrumb segment — those are views, not places.
- **Command bars are page-specific, not entity-wide.** Confirmed against
  real Azure Portal markup: an Overview page's command bar shows
  Delete/Start/Stop, while a Settings-style page shows a completely
  different, page-relevant set of actions instead. `EntityLayout` has no
  `commandBar` prop for this reason — each tab's own content renders its
  own `CommandBar` (see `OverviewTab.tsx` for the Delete button pattern).
  A tab's content can freely combine plain content, its own `CommandBar`,
  an inner `Tabs` (Azure's "Pivot" — sub-views within one tab, the same
  component the two Create flows already use for Basics/Review), or both.

### Live vs. placeholder resource types

`hubConfig.ts` gives every resource type a `status: 'live' | 'placeholder'`.
A **live** type gets a real list/create/detail flow backed by a real API
(today: only Virtual Machines and Proxmox Hosts). A **placeholder** type
still gets a real nav entry — matching Azure's shape even for things
crowCloud can't do yet — but renders an honest "not available yet" state
(`PlaceholderResourceTypePage` / `NotAvailableTab`) with a disabled Create
button. Never fake data, never a form that silently does nothing.

### Conventions

- **UI kit only.** Build with `src/ui/*` components; never raw unstyled
  HTML for something that already has one, and never add a new component
  or icon library dependency. New icons go in `src/ui/icons.tsx`, matching
  the existing inline-SVG stroke style (20×20 viewBox, `currentColor`
  stroke, ~10-line paths).
- **Layout via `theme.css` utility classes** (`.az-page`,
  `.az-stack-col`/`.az-stack-row`, `.az-gap-*`, `.az-card`,
  `.az-placeholder`, `.az-alert*`) before reaching for inline `style={}`.
  Inline styles are for genuinely one-off sizing only (e.g. a form's
  `maxWidth`).
- **Project scoping**: `useCurrentProject()` (top-bar picker,
  `localStorage`-backed, mirrors the CLI's `crow context`) is the one
  source of "current project" for anything project-scoped (the
  `resources` table). Globally-scoped entities (`providers`) don't take a
  project at all.
- **`useResources`/`useResource`** are guarded with `enabled: project.length > 0`
  — always call them with `current ?? ''` rather than skipping the call
  when no project is selected.

## Code Style

- **No `unwrap()` or `expect()`** in library code. Use `?` or return a typed error.
- **No `todo!()` in production paths** — use it only in stubs that are clearly not yet implemented.
- **Comments only for non-obvious WHY**, not what: the code explains what, comments explain why a non-obvious decision was made.
- **`-D warnings` is always on** — code must compile with zero warnings.
- **RUSTFLAGS="-D warnings"** is set in CI; run `cargo clippy --all-targets` locally before pushing.
- Prefer `tracing::{info, warn, error}` over `println!` for any observable output in binary crates.
