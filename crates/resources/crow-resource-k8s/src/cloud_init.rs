use std::net::IpAddr;

use uuid::Uuid;

/// Renders a cloud-init network-config v2 (netplan) document for a static
/// address — duplicated from `crow-resource-vm`'s identical helper rather
/// than adding a cross-resource-crate dependency for ~15 lines; promote to
/// `crow-core` if a third resource type ends up needing it too.
pub fn render_network_config(
    ip: IpAddr,
    prefix_len: u8,
    gateway: IpAddr,
    dns: &[String],
) -> String {
    let gateway_key = if gateway.is_ipv6() {
        "gateway6"
    } else {
        "gateway4"
    };
    let mut doc = format!(
        "network:\n  version: 2\n  ethernets:\n    id0:\n      match:\n        name: \"en*\"\n      dhcp4: false\n      addresses:\n        - {ip}/{prefix_len}\n      {gateway_key}: {gateway}\n"
    );
    if !dns.is_empty() {
        doc.push_str("      nameservers:\n        addresses:\n");
        for addr in dns {
            doc.push_str(&format!("          - {addr}\n"));
        }
    }
    doc
}

/// Random pre-shared secret — used for both the K3s cluster join token and
/// the bootstrap-callback secret. 128 bits from UUID v4 is plenty for a
/// self-hosted, non-adversarial threat model; not pulling in a `rand`
/// dependency just for this.
pub fn generate_token() -> String {
    Uuid::new_v4().simple().to_string()
}

pub struct ControlPlaneScriptInput<'a> {
    /// Set explicitly via `hostnamectl` and `k3s server --node-name`, not
    /// left to the VM's own hostname — every node clones from the same
    /// template and keeps its baked-in hostname otherwise (Proxmox
    /// provisioning doesn't apply `CloudInitConfig.hostname`), so every
    /// node in a cluster would try to register under the same K3s node
    /// name and collide. Confirmed live: with this unset, all nodes
    /// registered as "ubuntu" and stomped on each other's reported IP.
    pub node_name: &'a str,
    /// e.g. `"v1.29.4+k3s1"` — empty string installs K3s's current stable.
    pub k3s_version: &'a str,
    pub cluster_token: &'a str,
    /// This node's own IP — used for `--tls-san` (so the kubeconfig's
    /// server URL is reachable from outside the VM, not just `127.0.0.1`)
    /// and to rewrite the kubeconfig's server address before handing it back.
    pub node_ip: &'a str,
    pub pod_cidr: &'a str,
    pub service_cidr: &'a str,
    /// Cilium LB-IPAM range for LoadBalancer services. L2 announcement mode
    /// only for v1 — BGP mode needs peer/ASN config this CRD doesn't model
    /// yet, so it isn't wired into the script generator.
    pub lb_pool_cidr: Option<&'a str>,
    pub monitoring: bool,
    /// Where this VM POSTs once the cluster is confirmed up, e.g.
    /// `https://crow-api.example/api/v1/internal/k8s-clusters/{id}/report`.
    pub callback_url: &'a str,
    pub bootstrap_secret: &'a str,
    /// Authorized on the `ubuntu` user before anything else runs, so a
    /// bootstrap failure is still debuggable afterward instead of leaving
    /// no way into the VM. `None` skips this entirely.
    pub debug_ssh_public_key: Option<&'a str>,
}

pub struct WorkerScriptInput<'a> {
    /// See `ControlPlaneScriptInput::node_name` — same reasoning.
    pub node_name: &'a str,
    pub k3s_version: &'a str,
    pub cluster_token: &'a str,
    pub control_plane_ip: &'a str,
    pub debug_ssh_public_key: Option<&'a str>,
}

/// Sets the node's actual hostname (not just K3s's `--node-name`) so
/// anything else that shells in or reads `hostname` sees something
/// meaningful too, and so `hostnamectl`/`/etc/hosts` don't silently
/// disagree with what K3s registered the node as.
fn render_hostname_setup(node_name: &str) -> String {
    format!(
        "hostnamectl set-hostname '{node_name}'\necho \"127.0.1.1 {node_name}\" >> /etc/hosts\n\n"
    )
}

/// Authorizes a debug key on the `ubuntu` user — placed first in the
/// script (before anything that can fail) so it's in place regardless of
/// what happens afterward. Empty string when there's no key to inject.
fn render_debug_key_injection(key: Option<&str>) -> String {
    match key {
        Some(key) => format!(
            "mkdir -p /home/ubuntu/.ssh\necho '{key}' >> /home/ubuntu/.ssh/authorized_keys\nchown -R ubuntu:ubuntu /home/ubuntu/.ssh\nchmod 700 /home/ubuntu/.ssh\nchmod 600 /home/ubuntu/.ssh/authorized_keys\n\n"
        ),
        None => String::new(),
    }
}

/// Renders the control plane's cloud-init `user_data` — installs K3s with
/// its bundled CNI/service-lb/ingress disabled, then Helm-installs Cilium
/// (CNI + LB-IPAM), Longhorn (storage), and optionally kube-prometheus-stack,
/// then reports success + the kubeconfig back to crow-api.
///
/// Live-tested against a real 3-node boot (1 control plane + 2 workers).
/// `k8sServiceHost` deliberately uses the control plane's real static IP,
/// not `127.0.0.1` — that only resolves for the control plane's own
/// cilium-agent; workers have no local apiserver proxy listening on 6443
/// at all, so pointing every node at loopback left every worker's
/// cilium-agent stuck retrying a connection that could never succeed
/// (confirmed live: `ss -tlnp` showed nothing bound to 6443 on a worker).
pub fn render_control_plane_script(input: &ControlPlaneScriptInput) -> String {
    let k3s_version_env = if input.k3s_version.is_empty() {
        String::new()
    } else {
        format!("INSTALL_K3S_VERSION='{}' ", input.k3s_version)
    };

    let lb_ipam = match input.lb_pool_cidr {
        Some(cidr) => format!(
            r#"
cat <<'EOF' | kubectl apply -f -
apiVersion: cilium.io/v2alpha1
kind: CiliumLoadBalancerIPPool
metadata:
  name: default
spec:
  blocks:
  - cidr: {cidr}
EOF
cat <<'EOF' | kubectl apply -f -
apiVersion: cilium.io/v2alpha1
kind: CiliumL2AnnouncementPolicy
metadata:
  name: default
spec:
  loadBalancerIPs: true
  interfaces:
  - ^en.+
  - ^eth[0-9]+
  nodeSelector: {{}}
EOF
"#
        ),
        None => String::new(),
    };

    let monitoring = if input.monitoring {
        r#"
helm repo add prometheus-community https://prometheus-community.github.io/helm-charts
helm repo update
helm install kube-prometheus-stack prometheus-community/kube-prometheus-stack \
  --namespace monitoring --create-namespace --wait --timeout 15m
"#
    } else {
        ""
    };

    let debug_key = render_debug_key_injection(input.debug_ssh_public_key);
    let hostname_setup = render_hostname_setup(input.node_name);

    format!(
        r#"#!/bin/bash
set -euo pipefail

{debug_key}{hostname_setup}export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y curl jq open-iscsi nfs-common

{k3s_version_env}curl -sfL https://get.k3s.io | sh -s - server \
  --node-name='{node_name}' \
  --token='{cluster_token}' \
  --tls-san='{node_ip}' \
  --flannel-backend=none \
  --disable-network-policy \
  --disable=traefik \
  --disable=servicelb \
  --cluster-cidr='{pod_cidr}' \
  --service-cidr='{service_cidr}' \
  --write-kubeconfig-mode=644

until /usr/local/bin/k3s kubectl get nodes >/dev/null 2>&1; do sleep 5; done
export KUBECONFIG=/etc/rancher/k3s/k3s.yaml

curl -fsSL https://raw.githubusercontent.com/helm/helm/main/scripts/get-helm-3 | bash

helm repo add cilium https://helm.cilium.io/
helm repo update
helm install cilium cilium/cilium --namespace kube-system \
  --set l2announcements.enabled=true \
  --set k8sServiceHost='{node_ip}' \
  --set k8sServicePort=6443 \
  --wait --timeout 15m
kubectl -n kube-system rollout status daemonset/cilium --timeout=900s
{lb_ipam}
helm repo add longhorn https://charts.longhorn.io
helm repo update
helm install longhorn longhorn/longhorn \
  --namespace longhorn-system --create-namespace --wait --timeout 15m
kubectl patch storageclass longhorn \
  -p '{{"metadata": {{"annotations":{{"storageclass.kubernetes.io/is-default-class":"true"}}}}}}'
{monitoring}
KUBECONFIG_CONTENT="$(sed 's/127.0.0.1/{node_ip}/' /etc/rancher/k3s/k3s.yaml)"
curl -sf -X POST '{callback_url}' \
  -H 'Content-Type: application/json' \
  -H 'X-Bootstrap-Secret: {bootstrap_secret}' \
  -d "$(jq -n --arg kubeconfig "$KUBECONFIG_CONTENT" '{{kubeconfig: $kubeconfig}}')"
"#,
        debug_key = debug_key,
        hostname_setup = hostname_setup,
        node_name = input.node_name,
        k3s_version_env = k3s_version_env,
        cluster_token = input.cluster_token,
        node_ip = input.node_ip,
        pod_cidr = input.pod_cidr,
        service_cidr = input.service_cidr,
        lb_ipam = lb_ipam,
        monitoring = monitoring,
        callback_url = input.callback_url,
        bootstrap_secret = input.bootstrap_secret,
    )
}

/// Renders a worker's cloud-init `user_data`. Deliberately doesn't wait on
/// the control plane being ready before running — `get.k3s.io`'s agent
/// install retries connecting to `K3S_URL` on its own, so workers can be
/// created at the same time as the control plane rather than needing the
/// operator to sequence them across multiple reconciles.
pub fn render_worker_script(input: &WorkerScriptInput) -> String {
    let k3s_version_env = if input.k3s_version.is_empty() {
        String::new()
    } else {
        format!("INSTALL_K3S_VERSION='{}' ", input.k3s_version)
    };

    let debug_key = render_debug_key_injection(input.debug_ssh_public_key);
    let hostname_setup = render_hostname_setup(input.node_name);

    format!(
        r#"#!/bin/bash
set -euo pipefail

{debug_key}{hostname_setup}{k3s_version_env}curl -sfL https://get.k3s.io | K3S_URL='https://{control_plane_ip}:6443' K3S_TOKEN='{cluster_token}' sh -s - agent --node-name='{node_name}'
"#,
        debug_key = debug_key,
        hostname_setup = hostname_setup,
        node_name = input.node_name,
        k3s_version_env = k3s_version_env,
        control_plane_ip = input.control_plane_ip,
        cluster_token = input.cluster_token,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cp_input() -> ControlPlaneScriptInput<'static> {
        ControlPlaneScriptInput {
            node_name: "my-cluster-cp",
            k3s_version: "v1.29.4+k3s1",
            cluster_token: "tok-123",
            node_ip: "10.20.0.10",
            pod_cidr: "10.42.0.0/16",
            service_cidr: "10.43.0.0/16",
            lb_pool_cidr: Some("10.20.0.200/29"),
            monitoring: false,
            callback_url: "https://crow-api.local/api/v1/internal/k8s-clusters/abc/report",
            bootstrap_secret: "secret-xyz",
            debug_ssh_public_key: None,
        }
    }

    #[test]
    fn control_plane_script_pins_the_requested_k3s_version() {
        let doc = render_control_plane_script(&cp_input());
        assert!(doc.contains("INSTALL_K3S_VERSION='v1.29.4+k3s1'"));
    }

    #[test]
    fn control_plane_script_sets_a_unique_node_name() {
        // Every node clones from the same template and keeps its baked-in
        // hostname unless the script itself overrides it — without this,
        // every node in a cluster registers under the same K3s node name
        // and collides (confirmed live).
        let doc = render_control_plane_script(&cp_input());
        assert!(doc.contains("hostnamectl set-hostname 'my-cluster-cp'"));
        assert!(doc.contains("--node-name='my-cluster-cp'"));
        // Set before anything that can fail under `set -euo pipefail`.
        assert!(doc.find("hostnamectl").unwrap() < doc.find("apt-get update").unwrap());
    }

    #[test]
    fn control_plane_script_points_cilium_at_the_real_node_ip_not_loopback() {
        // 127.0.0.1:6443 only resolves on the control plane itself — workers
        // have no local apiserver proxy, so every worker's cilium-agent
        // would retry a connection that can never succeed (confirmed live).
        let doc = render_control_plane_script(&cp_input());
        assert!(doc.contains("--set k8sServiceHost='10.20.0.10'"));
        assert!(!doc.contains("k8sServiceHost=127.0.0.1"));
    }

    #[test]
    fn control_plane_script_omits_version_pin_when_empty() {
        let mut input = cp_input();
        input.k3s_version = "";
        let doc = render_control_plane_script(&input);
        assert!(!doc.contains("INSTALL_K3S_VERSION"));
    }

    #[test]
    fn control_plane_script_gives_helm_installs_a_generous_timeout() {
        // Helm's own default --wait timeout (5m) isn't enough under software
        // CPU emulation (kvm=0) — Longhorn in particular reliably blew past
        // it during live testing, aborting the whole script via `set -e`.
        let doc = render_control_plane_script(&cp_input());
        assert!(doc.contains("helm install cilium cilium/cilium"));
        assert!(doc.contains("--wait --timeout 15m"));
        assert_eq!(doc.matches("--timeout 15m").count(), 2);
    }

    #[test]
    fn control_plane_script_disables_bundled_cni_and_lb() {
        let doc = render_control_plane_script(&cp_input());
        assert!(doc.contains("--flannel-backend=none"));
        assert!(doc.contains("--disable=traefik"));
        assert!(doc.contains("--disable=servicelb"));
    }

    #[test]
    fn control_plane_script_includes_lb_ipam_pool_when_set() {
        let doc = render_control_plane_script(&cp_input());
        assert!(doc.contains("CiliumLoadBalancerIPPool"));
        assert!(doc.contains("cidr: 10.20.0.200/29"));
    }

    #[test]
    fn control_plane_script_omits_lb_ipam_when_unset() {
        let mut input = cp_input();
        input.lb_pool_cidr = None;
        let doc = render_control_plane_script(&input);
        assert!(!doc.contains("CiliumLoadBalancerIPPool"));
    }

    #[test]
    fn control_plane_script_installs_monitoring_only_when_opted_in() {
        let doc = render_control_plane_script(&cp_input());
        assert!(!doc.contains("kube-prometheus-stack"));

        let mut input = cp_input();
        input.monitoring = true;
        let doc = render_control_plane_script(&input);
        assert!(doc.contains("kube-prometheus-stack"));
    }

    #[test]
    fn control_plane_script_reports_back_with_the_bootstrap_secret() {
        let doc = render_control_plane_script(&cp_input());
        assert!(doc.contains("https://crow-api.local/api/v1/internal/k8s-clusters/abc/report"));
        assert!(doc.contains("X-Bootstrap-Secret: secret-xyz"));
    }

    #[test]
    fn worker_script_joins_using_the_shared_token() {
        let doc = render_worker_script(&WorkerScriptInput {
            node_name: "my-cluster-w0",
            k3s_version: "v1.29.4+k3s1",
            cluster_token: "tok-123",
            control_plane_ip: "10.20.0.10",
            debug_ssh_public_key: None,
        });
        assert!(doc.contains("K3S_URL='https://10.20.0.10:6443'"));
        assert!(doc.contains("K3S_TOKEN='tok-123'"));
        assert!(doc.contains("sh -s - agent"));
        assert!(doc.contains("--node-name='my-cluster-w0'"));
        assert!(doc.contains("hostnamectl set-hostname 'my-cluster-w0'"));
    }

    #[test]
    fn generate_token_produces_distinct_values() {
        assert_ne!(generate_token(), generate_token());
    }

    #[test]
    fn control_plane_script_authorizes_the_debug_key_when_set() {
        let mut input = cp_input();
        input.debug_ssh_public_key = Some("ssh-ed25519 AAAA... test@example");
        let doc = render_control_plane_script(&input);
        assert!(doc.contains("ssh-ed25519 AAAA... test@example"));
        assert!(doc.contains("/home/ubuntu/.ssh/authorized_keys"));
        // Placed before anything that can fail under `set -euo pipefail`.
        assert!(doc.find("authorized_keys").unwrap() < doc.find("apt-get update").unwrap());
    }

    #[test]
    fn control_plane_script_omits_debug_key_block_when_unset() {
        let doc = render_control_plane_script(&cp_input());
        assert!(!doc.contains("authorized_keys"));
    }

    #[test]
    fn worker_script_authorizes_the_debug_key_when_set() {
        let doc = render_worker_script(&WorkerScriptInput {
            node_name: "my-cluster-w0",
            k3s_version: "v1.29.4+k3s1",
            cluster_token: "tok-123",
            control_plane_ip: "10.20.0.10",
            debug_ssh_public_key: Some("ssh-ed25519 AAAA... test@example"),
        });
        assert!(doc.contains("ssh-ed25519 AAAA... test@example"));
        assert!(doc.contains("/home/ubuntu/.ssh/authorized_keys"));
    }
}
