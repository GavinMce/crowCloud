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
# Optional, set together by `crow-cli iso proxmox build`'s
# --vyos-uplink-interface/--vyos-ssh-private-key: lets the operator's
# ExposedEndpoint controller configure itself against VyOS automatically
# instead of that being a manual `helm upgrade --set operator.vyos.*`
# step after the fact (confirmed live: every fresh deployment otherwise
# starts with that controller silently disabled).
VYOS_HOST="${VYOS_HOST:-}"
VYOS_UPLINK_INTERFACE="${VYOS_UPLINK_INTERFACE:-}"
VYOS_SSH_KEY_PATH="${VYOS_SSH_KEY_PATH:-}"
# Optional, set by the seed VM's own cloud-init (crow-cli iso proxmox
# build's post-install hook, when it self-elects as the fleet seed):
# the physical Proxmox host's own identity and a freshly-generated API
# token for it, so this seed can register that host as crowCloud's
# first provider automatically instead of a manual `crow provider
# add-proxmox` step after the fact.
PROXMOX_HOST_MAC="${PROXMOX_HOST_MAC:-}"
PROXMOX_HOST_NODE_NAME="${PROXMOX_HOST_NODE_NAME:-}"
PROXMOX_HOST_STORAGE="${PROXMOX_HOST_STORAGE:-}"
PROXMOX_HOST_BRIDGE="${PROXMOX_HOST_BRIDGE:-}"
PROXMOX_HOST_TOKEN_SECRET="${PROXMOX_HOST_TOKEN_SECRET:-}"
PROXMOX_HOST_MGMT_IP="${PROXMOX_HOST_MGMT_IP:-}"
PROXMOX_HOST_URL="${PROXMOX_HOST_URL:-}"
PROXMOX_HOST_TOKEN_ID="${PROXMOX_HOST_TOKEN_ID:-}"
# Fleet-wide BGP facts (not per-node like the vars above) -- needed so
# crowCloud can create Proxmox SDN's one-time EVPN controller pointed at
# VyOS (see crow-provider-proxmox::network) alongside the provider itself.
PROXMOX_HOST_BGP_ASN="${PROXMOX_HOST_BGP_ASN:-}"
PROXMOX_HOST_BGP_ROUTE_REFLECTOR_IP="${PROXMOX_HOST_BGP_ROUTE_REFLECTOR_IP:-}"

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

# Confirmed live: k3s's own `kubectl` is a symlink to the k3s binary
# itself, which special-cases its argv[0] to auto-find
# /etc/rancher/k3s/k3s.yaml with no KUBECONFIG needed -- so the wait
# loop right below this works fine either way. `helm`, installed
# separately just below, is a completely different binary with no such
# special-casing: it only ever checks --kubeconfig/KUBECONFIG/
# ~/.kube/config, none of which point here by default. Every `helm`
# command after this failed with "Kubernetes cluster unreachable" until
# this was set, despite kubectl itself working the entire time.
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml

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
# Confirmed live: the API Service defaults to ClusterIP, which is fine for
# browser users (the frontend's nginx proxies /api/ to it internally) but
# leaves `crow` CLI users -- who talk to the API directly, bypassing the
# frontend entirely -- with no way to reach it at all. NodePort regardless
# of CROW_DOMAIN/ingress, since ingress here only routes the frontend.
HELM_ARGS+=(--set api.service.type=NodePort)
if [ -n "$VYOS_HOST" ] && [ -n "$VYOS_UPLINK_INTERFACE" ] && [ -n "$VYOS_SSH_KEY_PATH" ]; then
  HELM_ARGS+=(
    --set operator.vyos.host="$VYOS_HOST"
    --set operator.vyos.uplinkInterface="$VYOS_UPLINK_INTERFACE"
    --set-file operator.vyos.sshPrivateKey="$VYOS_SSH_KEY_PATH"
  )
fi
helm "${HELM_ARGS[@]}" --wait

# Confirmed live: this used to hardcode the API's internal ClusterIP port
# (8080), which was never actually reachable at that address -- querying
# the real nodePort instead. Resolved once here since both the provider
# self-registration call below and the final printed instructions need it.
API_NODE_PORT="$(kubectl get svc -n "$NAMESPACE" crowcloud-api -o jsonpath='{.spec.ports[0].nodePort}')"

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
  # Confirmed live: with no -h, psql defaults to CNPG's own internal
  # instance-manager socket (/controller/run/.s.PGSQL.5432) rather than
  # a normal Postgres listener -- that socket uses peer authentication,
  # which compares the OS user running the client against the requested
  # role name. `kubectl exec` runs as the container's default user, not
  # `crowcloud`, so it always failed with "Peer authentication failed
  # for user crowcloud". Forcing -h 127.0.0.1 hits the real Postgres TCP
  # listener instead, authenticating with the actual app-user password
  # from CNPG's own generated <cluster>-app secret.
  PG_PASSWORD="$(kubectl get secret -n "$NAMESPACE" "${PG_CLUSTER}-app" -o jsonpath='{.data.password}' | base64 -d)"
  kubectl exec -n "$NAMESPACE" "$PG_POD" -- env PGPASSWORD="$PG_PASSWORD" psql -h 127.0.0.1 -U crowcloud -d crowcloud -c \
    "INSERT INTO fleet_secrets (secret, label) VALUES ('${CROW_FLEET_SECRET}', 'seed-bootstrap') ON CONFLICT (secret) DO NOTHING;"
fi

if [ -n "$PROXMOX_HOST_MAC" ]; then
  echo "==> Registering the physical Proxmox host as crowCloud's first provider"
  # This is the seed VM calling back into the crowCloud instance it just
  # stood up on itself, over the NodePort just resolved above -- not a
  # separate host joining an existing fleet (that's the same
  # /hosts/register endpoint, but called from a *different* host's own
  # post-install hook, with no proxmox_* fields since a provider already
  # exists by then). See host_bootstrap.rs's own [] branch.
  PROVIDER_REGISTER_PAYLOAD=$(cat <<JSON
{"mac_address":"${PROXMOX_HOST_MAC}","node_name":"${PROXMOX_HOST_NODE_NAME}","default_storage":"${PROXMOX_HOST_STORAGE}","default_bridge":"${PROXMOX_HOST_BRIDGE}","management_ip":"${PROXMOX_HOST_MGMT_IP}","proxmox_url":"${PROXMOX_HOST_URL}","proxmox_token_id":"${PROXMOX_HOST_TOKEN_ID}","proxmox_token_secret":"${PROXMOX_HOST_TOKEN_SECRET}","bgp_asn":${PROXMOX_HOST_BGP_ASN},"bgp_route_reflector_ip":"${PROXMOX_HOST_BGP_ROUTE_REFLECTOR_IP}"}
JSON
)
  set +e
  PROVIDER_HTTP_CODE="$(curl -s -o /tmp/crowcloud-provider-register-response.json -w '%{http_code}' \
    --connect-timeout 5 --max-time 15 \
    -X POST "http://127.0.0.1:${API_NODE_PORT}/api/v1/internal/hosts/register" \
    -H "X-Fleet-Secret: ${CROW_FLEET_SECRET}" \
    -H 'Content-Type: application/json' \
    -d "${PROVIDER_REGISTER_PAYLOAD}")"
  PROVIDER_CURL_EXIT=$?
  set -e
  if [ "$PROVIDER_CURL_EXIT" -ne 0 ] || { [ "$PROVIDER_HTTP_CODE" != "200" ] && [ "$PROVIDER_HTTP_CODE" != "201" ]; }; then
    echo "    provider self-registration failed (HTTP ${PROVIDER_HTTP_CODE:-000})" >&2
    cat /tmp/crowcloud-provider-register-response.json >&2 || true
  fi
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
    crow login --server http://$NODE_IP:$API_NODE_PORT

EOF
