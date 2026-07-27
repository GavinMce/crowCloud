#!/usr/bin/env bash
# Day-0 bootstrap: installs K3s on this machine and deploys crowCloud.
# Run this on a fresh VM (or the host itself) that will become the
# management cluster — e.g. a small VM on your Proxmox box. Once this
# finishes, open the printed URL, create the admin account, and add that
# Proxmox host as a Cloud Host from the UI.
#
# Also runnable unattended (no repo clone required, no admin account
# created yet) as part of the seed-election flow (#67) — set
# CROW_FLEET_SECRET so the freshly-deployed instance immediately accepts
# self-registration from every other host built with the same secret,
# without needing an admin to log in and mint one via the UI first.
set -euo pipefail

CROW_VERSION="${CROW_VERSION:-latest}"
# Optional: set to a real hostname you control to enable Ingress with
# TLS-ready host routing instead of the default NodePort exposure. Requires
# an ingress controller and DNS pointed at this cluster — neither is set up
# by this script.
CROW_DOMAIN="${CROW_DOMAIN:-}"
# Optional: if set, registered as a valid fleet secret (#65) once
# crowCloud is up, so hosts self-registering with this secret (#66/#67)
# work immediately with no manual "create a fleet secret" step.
CROW_FLEET_SECRET="${CROW_FLEET_SECRET:-}"

NAMESPACE=crow-system
# Only used for the local-checkout preference below, not for fetching
# the chart -- that comes from the same GHCR OCI artifact the release
# pipeline (cd_publish.yml) already publishes the Docker images to,
# not a fresh git clone of the source repo (confirmed live: the chart
# was previously fetched by cloning the whole repo at whatever `main`
# happened to be, unpinned to CROW_VERSION -- a real version-skew risk
# against the pinned container image tags, on top of depending on
# GitHub reachability as well as GHCR).
CHART_OCI="oci://ghcr.io/gavinmce/charts/crowcloud"

echo "==> Checking prerequisites"
if [ "$(id -u)" -ne 0 ]; then
  echo "This script installs a system service (k3s) and must be run as root (sudo)." >&2
  exit 1
fi
for cmd in curl openssl; do
  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Missing required command: $cmd" >&2
    exit 1
  fi
done

echo "==> Installing K3s (management cluster)"
curl -sfL https://get.k3s.io | sh -s - \
  --disable traefik \
  --disable servicelb

echo "==> Waiting for K3s to be ready"
until kubectl get nodes 2>/dev/null | grep -q "Ready"; do sleep 2; done

echo "==> Installing Helm"
curl https://raw.githubusercontent.com/helm/helm/main/scripts/get-helm-3 | bash

echo "==> Installing CloudNativePG (Postgres operator)"
helm repo add cnpg https://cloudnative-pg.github.io/charts
helm upgrade --install cnpg cnpg/cloudnative-pg \
  --namespace cnpg-system \
  --create-namespace \
  --wait

if [ -d "./charts/crowcloud" ]; then
  echo "==> Using the local ./charts/crowcloud checkout (unpublished changes take priority)"
  CHART_PATH="./charts/crowcloud"
else
  echo "==> Pulling the crowCloud Helm chart from GHCR (version: $CROW_VERSION)"
  CHART_DIR="$(mktemp -d)"
  # Confirmed against Helm's own OCI resolution source
  # (pkg/registry/client.go, ValidateReference): omitting --version
  # entirely lists the OCI repo's tags, semver-sorts them, and resolves
  # to the newest -- but passing the literal string "latest" as
  # --version fails outright ("improper constraint: latest"), since
  # OCI chart tags are never aliased to "latest" the way Docker image
  # tags are. Must omit the flag, not pass it, for the default case.
  if [ "$CROW_VERSION" = "latest" ]; then
    helm pull "$CHART_OCI" --untar --untardir "$CHART_DIR"
  else
    helm pull "$CHART_OCI" --version "$CROW_VERSION" --untar --untardir "$CHART_DIR"
  fi
  CHART_PATH="$CHART_DIR/crowcloud"
fi

echo "==> Deploying crowCloud (version: $CROW_VERSION)"
HELM_ARGS=(
  upgrade --install crowcloud "$CHART_PATH"
  --namespace "$NAMESPACE"
  --create-namespace
  --set api.tag="$CROW_VERSION"
  --set operator.tag="$CROW_VERSION"
  --set frontend.tag="$CROW_VERSION"
  --set api.env.JWT_SECRET="$(openssl rand -hex 32)"
)
if [ -n "$CROW_DOMAIN" ]; then
  HELM_ARGS+=(--set ingress.enabled=true --set "ingress.host=$CROW_DOMAIN")
else
  HELM_ARGS+=(--set frontend.service.type=NodePort)
fi
helm "${HELM_ARGS[@]}" --wait

if [ -n "$CROW_FLEET_SECRET" ]; then
  echo "==> Registering fleet secret for automatic host self-registration"
  # CNPG's pod-labeling convention (cnpg.io/cluster=<name>, role=primary)
  # verified live against a real CNPG 1-instance Cluster, not just
  # documented behavior -- confirmed the primary pod carries both
  # cnpg.io/instanceRole=primary and role=primary.
  PG_CLUSTER="$(kubectl get cluster.postgresql.cnpg.io -n "$NAMESPACE" -o jsonpath='{.items[0].metadata.name}')"
  echo "    waiting for Postgres primary (${PG_CLUSTER})"
  until kubectl get pod -n "$NAMESPACE" -l "cnpg.io/cluster=${PG_CLUSTER},role=primary" 2>/dev/null | grep -q Running; do
    sleep 2
  done
  PG_POD="$(kubectl get pod -n "$NAMESPACE" -l "cnpg.io/cluster=${PG_CLUSTER},role=primary" -o jsonpath='{.items[0].metadata.name}')"
  kubectl exec -n "$NAMESPACE" "$PG_POD" -- psql -U crowcloud -d crowcloud -c \
    "INSERT INTO fleet_secrets (secret, label) VALUES ('${CROW_FLEET_SECRET}', 'seed-bootstrap') ON CONFLICT (secret) DO NOTHING;"
fi

echo "==> Resolving the crowCloud URL"
NODE_IP="$(hostname -I | awk '{print $1}')"
if [ -n "$CROW_DOMAIN" ]; then
  URL="http://$CROW_DOMAIN"
else
  NODE_PORT="$(kubectl get svc -n "$NAMESPACE" crowcloud-frontend -o jsonpath='{.spec.ports[0].nodePort}')"
  URL="http://$NODE_IP:$NODE_PORT"
fi

cat <<EOF

crowCloud is running.

  Open $URL in your browser to create the admin account and add your
  first Cloud Host (e.g. Proxmox).

  Prefer the CLI?
    Download crow-cli from https://github.com/GavinMce/crowCloud/releases
    crow login --server http://$NODE_IP:8080

EOF
