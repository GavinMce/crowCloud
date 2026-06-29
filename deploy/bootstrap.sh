#!/usr/bin/env bash
# Day-0 bootstrap: installs K3s on this machine and deploys crowCloud.
# Run this on a fresh VM on Proxmox that will become the management cluster.
set -euo pipefail

CROW_DOMAIN="${CROW_DOMAIN:-}"
CROW_VERSION="${CROW_VERSION:-latest}"

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

echo "==> Deploying crowCloud"
helm upgrade --install crowcloud ./charts/crowcloud \
  --namespace crow-system \
  --create-namespace \
  --set api.env.JWT_SECRET="$(openssl rand -hex 32)" \
  --wait

echo ""
echo "crowCloud is running."
echo "Install the CLI: cargo install --path crates/crow-cli"
echo "Then: crow login --server http://<this-vm-ip>:8080"
