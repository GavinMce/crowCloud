#!/usr/bin/env bash
# Day-0 bootstrap: installs K3s on this machine and deploys crowCloud.
# Run this on a fresh VM (or the host itself) that will become the
# management cluster — e.g. a small VM on your Proxmox box. Once this
# finishes, open the printed URL, create the admin account, and add that
# Proxmox host as a Cloud Host from the UI.
set -euo pipefail

CROW_VERSION="${CROW_VERSION:-latest}"
# Optional: set to a real hostname you control to enable Ingress with
# TLS-ready host routing instead of the default NodePort exposure. Requires
# an ingress controller and DNS pointed at this cluster — neither is set up
# by this script.
CROW_DOMAIN="${CROW_DOMAIN:-}"

NAMESPACE=crow-system

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

echo "==> Deploying crowCloud (version: $CROW_VERSION)"
HELM_ARGS=(
  upgrade --install crowcloud ./charts/crowcloud
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
    cargo install --path crates/crow-cli
    crow login --server http://$NODE_IP:8080

EOF
