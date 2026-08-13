# Development Cycle

Two different loops, for two different purposes. Use the release cycle for
anything going to users; use the dev-build cycle for iterating against a
live fleet (e.g. the hardware validation in
[#95](https://github.com/GavinMce/crowCloud/issues/95)) without cutting a
release per change.

## Cluster access

Both loops end with `helm upgrade`/`kubectl` against the fleet's actual
cluster, which — unlike the crowCloud API/UI — has no NAT-forwarded port
through VyOS's uplink and isn't meant to get one (handing out kubeconfig
through crowCloud's own API would mean any API-authenticated user gets
cluster-admin by extension). The intended way in is the admin WireGuard
VPN terminating on VyOS — see
[`docs/hardware-setup.md`'s Step 2e](hardware-setup.md#2e-admin-vpn-access-optional)
to set it up and add yourself as a peer. Once connected, every command
below works exactly as if you were physically on the fabric.

## Release cycle (production)

Fully automated already — see the root [`CLAUDE.md`](../CLAUDE.md#release-flow)
for the complete mechanics. In short:

1. Push conventional-commit changes to `main`.
2. `cd_release.yml` (release-plz) opens/updates a "Release crowCloud
   vX.Y.Z" PR.
3. Merging that PR tags the release (e.g. `v0.3.0`).
4. The tag push triggers `cd_publish.yml`: Docker images → GHCR (tagged
   both with the version, e.g. `0.3.0`, and the floating `latest`), CLI
   binaries, Helm chart → GHCR OCI (versioned `0.3.0`), GitHub Release.
5. Upgrade the fleet, **pinning the version explicitly rather than
   relying on `operator.tag`'s `latest` default**:

   ```bash
   VERSION=$(git tag -l 'v*.*.*' --sort=-v:refname | head -1 | tr -d v)

   helm upgrade crowcloud oci://ghcr.io/gavinmce/charts/crowcloud \
     --version "$VERSION" \
     --reuse-values \
     --set api.tag="$VERSION" \
     --set operator.tag="$VERSION" \
     --set frontend.tag="$VERSION"
   ```

   Why pin instead of just re-running `helm upgrade` against `latest`:
   with `imagePullPolicy: IfNotPresent` (the chart's default), a floating
   tag like `latest` is indistinguishable to Helm/Kubernetes before and
   after a release — the image reference string in the rendered
   Deployment spec hasn't changed, so nothing detects that its remote
   content has, and no rollout happens even though a newer image exists
   on GHCR. A version-pinned tag is a genuinely different string each
   release, so Helm always produces a real spec diff and Kubernetes
   always rolls — same reasoning the dev-build cycle's SHA tags rely on
   below, and the same pattern `crow-cli/deploy/bootstrap.sh` already
   uses for a fresh install (`--set operator.tag="$CROW_VERSION"` etc.).
   No separate CRD-upgrade step needed either — `crow-operator` re-applies
   its own CRDs at startup (`main.rs::install_crds`), so once the new pod
   is running, any CRD changes land automatically.

6. Confirm the rollout the same way as the dev-build cycle (step 4
   below): `kubectl -n crow-system rollout status
   deployment/crowcloud-operator`.

This is the right path for anything stable enough to ship — it's also the
only path that produces a real, versioned Helm chart. It's too slow for
"change one line in the operator, see if it fixes the bug" though — that's
what the next section is for.

## Dev-build cycle (iterating against real hardware)

`.github/workflows/dev_build.yml` — manually triggered, builds and pushes
one component's image to GHCR tagged with the triggering commit's short
SHA (not a floating tag like `latest`/`dev`), so `imagePullPolicy:
IfNotPresent` never serves a stale cached image: every dev build gets its
own never-before-seen tag.

**1. Trigger a build** (from a branch with the change you want to test):

```bash
gh workflow run dev_build.yml --ref <branch> -f component=crow-operator
```

Or use the Actions tab → "Dev Build" → "Run workflow" in the GitHub UI.
`component` is one of `crow-operator`, `crow-api`, `crow-frontend`, or
`all`.

**2. Get the tag.** The workflow's job summary prints the exact
`helm upgrade` command once it finishes, including the resolved SHA tag —
open the run and check its Summary tab in the GitHub UI (`gh run list
--workflow=dev_build.yml --limit 1` to find it from the CLI, then open
the printed URL in a browser; job summaries aren't readable via `gh run
view --log`, only the web UI).

**3. Deploy it** (over the VPN from "Cluster access" above, or however
else you reach the cluster). Check the currently-installed chart version
first so only the image tag changes, not the chart itself:

```bash
helm list -n crow-system   # note the installed chart version
helm upgrade crowcloud oci://ghcr.io/gavinmce/charts/crowcloud \
  --version <that version> \
  --reuse-values --set operator.tag=<sha-from-step-2>
```

**4. Confirm the rollout:**

```bash
kubectl -n crow-system rollout status deployment/crowcloud-operator
kubectl -n crow-system logs -l app.kubernetes.io/component=operator -f
```

Repeat from step 1 for each iteration. Once a change is actually working,
land it through the normal PR → `main` → release flow above rather than
leaving the fleet pinned to a dev SHA tag indefinitely.

### Why not `crow-vps-agent`?

It isn't part of this Helm chart at all — it runs standalone on a rented
VPS (see `crow-vps-agent`'s own description in the root `CLAUDE.md`), not
inside the crowCloud cluster, so there's no `helm upgrade` step for it the
way there is for the other three. Updating it is a separate, manual
process against whatever VPS is running it.
