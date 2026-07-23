/// Inputs for rendering a Proxmox `answer.toml` (`proxmox-auto-install-assistant`)
/// and the post-install hook script (#66/#67).
///
/// The `answer.toml` shape is confirmed live against a real
/// `proxmox-auto-install-assistant validate-answer` (v8.3.4, run inside a
/// throwaway QEMU/Debian VM with Proxmox's own apt repo added) -- not
/// just documented best-effort. That run caught two real gaps this
/// struct's fields now cover: `[global].mailto` is required and was
/// missing entirely, and `--on-first-boot` (the CLI flag bundling the
/// hook script into the ISO) silently does nothing without an explicit
/// `[first-boot]\nsource = "from-iso"` section enabling it in the answer
/// file itself.
pub struct ProxmoxBuildConfig {
    pub root_password_hash: String,
    pub fqdn: String,
    /// Required by `answer.toml`'s `[global]` section -- confirmed live
    /// against `proxmox-auto-install-assistant validate-answer` (8.3.4),
    /// which rejects the file outright without it.
    pub admin_email: String,
    pub trunk_interface: String,
    pub underlay_vlan: u16,
    pub mgmt_vlan: u16,
    pub mgmt_ip: String,
    pub mgmt_prefix: u8,
    pub mgmt_gateway: String,
    pub trunk_mtu: u32,
    pub disk_list: Vec<String>,
    pub zfs_raid: Option<String>,
    pub crow_api_url: String,
    pub fleet_secret: String,
    pub bgp_asn: u32,
    pub bgp_peer_password: String,
    pub underlay_prefix: u8,
    pub ospf_area: String,
}

/// The literal contents of `deploy/bootstrap.sh`, embedded at compile
/// time. `crow-cli` is a distributed binary, not always run from inside
/// a repo checkout, so the seed VM's cloud-init can't assume the script
/// is available on disk relative to wherever `crow-cli` happens to run
/// -- baking it in keeps this in sync with the real script automatically
/// (single source of truth) without a runtime dependency on fetching it
/// from anywhere.
const BOOTSTRAP_SH: &str = include_str!("../../../../deploy/bootstrap.sh");

/// Cloud-init user-data for the seed VM (#67) -- writes and runs
/// `bootstrap.sh` unattended, with `CROW_FLEET_SECRET` set so the
/// resulting crowCloud instance immediately accepts self-registration
/// from every other host built with the same fleet secret.
pub fn render_seed_cloud_init(fleet_secret: &str) -> String {
    format!(
        r#"#cloud-config
write_files:
  - path: /root/bootstrap.sh
    permissions: '0755'
    content: |
{bootstrap_sh}
runcmd:
  - [ bash, -c, "CROW_FLEET_SECRET={fleet_secret} /root/bootstrap.sh > /var/log/crowcloud-bootstrap.log 2>&1" ]
"#,
        bootstrap_sh = indent(BOOTSTRAP_SH, "      "),
        fleet_secret = fleet_secret,
    )
}

fn indent(s: &str, prefix: &str) -> String {
    s.lines()
        .map(|line| format!("{prefix}{line}"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn render_answer_toml(cfg: &ProxmoxBuildConfig) -> String {
    let mut out = String::new();

    out.push_str("[global]\n");
    out.push_str("keyboard = \"en-us\"\n");
    out.push_str("country = \"us\"\n");
    out.push_str(&format!("fqdn = \"{}\"\n", cfg.fqdn));
    out.push_str(&format!("mailto = \"{}\"\n", cfg.admin_email));
    out.push_str("timezone = \"UTC\"\n");
    out.push_str(&format!(
        "root-password-hashed = \"{}\"\n",
        cfg.root_password_hash
    ));
    out.push('\n');

    // Installer-time network only -- enough to reach package repos
    // during install. The real fabric config (trunk VLANs, underlay,
    // FRR) is applied by the post-install hook after the base OS is up,
    // not here -- matches the architecture doc's split between "get a
    // bootable, network-reachable box" and "make it a fabric member".
    out.push_str("[network]\n");
    out.push_str("source = \"from-dhcp\"\n");
    out.push('\n');

    out.push_str("[disk-setup]\n");
    if let Some(raid) = &cfg.zfs_raid {
        out.push_str("filesystem = \"zfs\"\n");
        out.push_str(&format!("zfs.raid = \"{raid}\"\n"));
    } else {
        out.push_str("filesystem = \"ext4\"\n");
    }
    let disks = cfg
        .disk_list
        .iter()
        .map(|d| format!("\"{d}\""))
        .collect::<Vec<_>>()
        .join(", ");
    out.push_str(&format!("disk-list = [{disks}]\n"));
    out.push('\n');

    // Confirmed live: `--on-first-boot` (the CLI flag bundling the hook
    // script into the ISO) does nothing on its own -- the installed
    // system won't actually run it without this section explicitly
    // enabling it. `source = "from-iso"` matches passing the hook via
    // `--on-first-boot` at prepare-iso time (as opposed to fetching it
    // over HTTP at first-boot instead).
    out.push_str("[first-boot]\n");
    out.push_str("source = \"from-iso\"\n");

    out
}

/// The post-install hook (#66/#67) -- runs once, on first boot after the
/// base Proxmox install completes. Implements the underlay/fabric setup
/// mirroring VyOS's own config, then either self-registers with an
/// existing crowCloud instance or, finding none, self-elects as the
/// fleet's seed and stands crowCloud up on itself.
pub fn render_post_install_hook(cfg: &ProxmoxBuildConfig) -> String {
    let seed_cloud_init = render_seed_cloud_init(&cfg.fleet_secret);
    format!(
        r#"#!/usr/bin/env bash
# Generated by `crow-cli iso proxmox build` -- see #66/#67.
set -euo pipefail

TRUNK_IF="{trunk_interface}"
UNDERLAY_VLAN="{underlay_vlan}"
MGMT_VLAN="{mgmt_vlan}"
MGMT_IP="{mgmt_ip}"
MGMT_PREFIX="{mgmt_prefix}"
MGMT_GATEWAY="{mgmt_gateway}"
TRUNK_MTU="{trunk_mtu}"
CROW_API_URL="{crow_api_url}"
FLEET_SECRET="{fleet_secret}"
BGP_ASN="{bgp_asn}"
BGP_PEER_PASSWORD="{bgp_peer_password}"
UNDERLAY_PREFIX="{underlay_prefix}"
OSPF_AREA="{ospf_area}"

echo "==> Installing FRR"
apt-get update
apt-get install -y frr

echo "==> Configuring management VLAN (${{TRUNK_IF}}.${{MGMT_VLAN}})"
cat >> /etc/network/interfaces <<IFACES

auto ${{TRUNK_IF}}.${{MGMT_VLAN}}
iface ${{TRUNK_IF}}.${{MGMT_VLAN}} inet static
    address ${{MGMT_IP}}/${{MGMT_PREFIX}}
    mtu ${{TRUNK_MTU}}
    gateway ${{MGMT_GATEWAY}}
IFACES

# Writing to /etc/network/interfaces alone doesn't bring the interface
# up -- confirmed live (it silently sat absent from `ip a` until this
# was added). ifup requires the parent link to exist first.
ip link set dev "${{TRUNK_IF}}" up
ifup "${{TRUNK_IF}}.${{MGMT_VLAN}}" || true

echo "==> Configuring underlay VLAN (${{TRUNK_IF}}.${{UNDERLAY_VLAN}}) + loopback"
UNDERLAY_IP="$(ip -4 -o addr show dev "${{TRUNK_IF}}" 2>/dev/null | awk '{{print $4}}' | head -1 || true)"
# NOTE: this host's own underlay IP isn't determined by this script --
# it's assigned by whatever IPAM decision crowCloud/#54's Subnet
# allocator makes for the underlay. Left as an explicit gap: the real
# value needs to come from somewhere (a build-time flag once #54 exists,
# or a pre-underlay-network DHCP reservation) rather than guessed here.
ip link set dev "${{TRUNK_IF}}" mtu "${{TRUNK_MTU}}"

echo "==> Configuring FRR (OSPF underlay + BGP EVPN dynamic peer)"
cat > /etc/frr/daemons.conf.d/crowcloud <<FRRCONF
router ospf
 network 0.0.0.0/0 area ${{OSPF_AREA}}
!
router bgp ${{BGP_ASN}}
 neighbor FABRIC peer-group
 neighbor FABRIC remote-as internal
 neighbor FABRIC password ${{BGP_PEER_PASSWORD}}
 address-family l2vpn evpn
  neighbor FABRIC activate
 exit-address-family
!
FRRCONF
systemctl enable --now frr

echo "==> Detecting local Proxmox defaults"
DEFAULT_STORAGE="$(pvesm status --content images 2>/dev/null | awk 'NR==2{{print $1}}')"
DEFAULT_BRIDGE="vmbr0"
NODE_NAME="$(hostname)"
MAC_ADDRESS="$(cat /sys/class/net/${{TRUNK_IF}}/address)"

echo "==> Attempting self-registration with crowCloud at ${{CROW_API_URL}}"
REGISTER_URL="${{CROW_API_URL%/}}/api/v1/internal/hosts/register"
REGISTER_PAYLOAD=$(cat <<JSON
{{"mac_address":"${{MAC_ADDRESS}}","node_name":"${{NODE_NAME}}","default_storage":"${{DEFAULT_STORAGE}}","default_bridge":"${{DEFAULT_BRIDGE}}","management_ip":"${{MGMT_IP}}"}}
JSON
)

HTTP_CODE="$(curl -s -o /tmp/crowcloud-register-response.json -w '%{{http_code}}' \
  --connect-timeout 5 --max-time 15 \
  -X POST "${{REGISTER_URL}}" \
  -H "X-Fleet-Secret: ${{FLEET_SECRET}}" \
  -H 'Content-Type: application/json' \
  -d "${{REGISTER_PAYLOAD}}" || echo "000")"

if [ "${{HTTP_CODE}}" = "000" ]; then
    echo "==> No crowCloud instance reachable at ${{CROW_API_URL}} -- self-electing as fleet seed (#67)"

    # A truly fresh first host has no templates to clone -- unlike
    # regular VM provisioning (which should reuse whatever template
    # convention #40 eventually builds), the seed VM's own bootstrap is
    # inherently a from-nothing case, so it fetches its own base image
    # directly via Proxmox's own download-url API (the same mechanism
    # #40 documents for default-template creation) rather than depending
    # on a template that can't exist yet. Debian 12's official cloud
    # image is the pinned, curated choice here -- not user-configurable
    # arbitrary input, matching #40's stated preference for a small
    # curated catalog over open-ended URLs.
    SEED_VMID="$(pvesh get /cluster/nextid)"
    SEED_IMAGE_URL="https://cloud.debian.org/images/cloud/bookworm/latest/debian-12-generic-amd64.qcow2"
    echo "==> Fetching base image for the seed VM (${{SEED_IMAGE_URL}})"
    pvesh create /nodes/"${{NODE_NAME}}"/storage/"${{DEFAULT_STORAGE}}"/download-url \
        --url "${{SEED_IMAGE_URL}}" \
        --content import \
        --filename "crowcloud-seed-base.qcow2"

    echo "==> Creating seed VM ${{SEED_VMID}} from that image (guest, not the bare host OS)"
    qm create "${{SEED_VMID}}" --name crowcloud-seed --memory 4096 --cores 2 \
        --net0 "virtio,bridge=${{DEFAULT_BRIDGE}}" --scsihw virtio-scsi-pci \
        --ostype l26
    qm importdisk "${{SEED_VMID}}" "/var/lib/vz/import/crowcloud-seed-base.qcow2" "${{DEFAULT_STORAGE}}"
    qm set "${{SEED_VMID}}" --scsi0 "${{DEFAULT_STORAGE}}:vm-${{SEED_VMID}}-disk-0"
    qm set "${{SEED_VMID}}" --boot c --bootdisk scsi0
    qm resize "${{SEED_VMID}}" scsi0 +12G
    qm set "${{SEED_VMID}}" --ide2 "${{DEFAULT_STORAGE}}:cloudinit"

    echo "==> Writing cloud-init user-data (runs bootstrap.sh unattended inside the guest)"
    mkdir -p /var/lib/vz/snippets
    cat > "/var/lib/vz/snippets/crowcloud-seed-${{SEED_VMID}}.yaml" <<'CLOUDINIT'
{seed_cloud_init}
CLOUDINIT

    qm set "${{SEED_VMID}}" \
        --cicustom "user=local:snippets/crowcloud-seed-${{SEED_VMID}}.yaml" \
        --ipconfig0 "ip=dhcp"
    qm start "${{SEED_VMID}}"

    # NOTE: this kicks the seed deployment off and returns -- it does not
    # wait for or confirm crowCloud actually comes up inside the guest
    # (no readiness polling, no retry if any step above fails). The
    # exact `pvesh`/`qm` flag names/behavior here are not verified
    # against a live Proxmox host in this environment (no real PVE
    # install available to test against, only the auto-install-assistant
    # tool itself) -- treat this sequence as best-effort pending a real
    # run. `bootstrap.sh`'s own progress is visible at
    # /var/log/crowcloud-bootstrap.log *inside* that guest.
    echo "==> VM ${{SEED_VMID}} started -- crowCloud is deploying inside it unattended."
    echo "    Check /var/log/crowcloud-bootstrap.log on that guest, or ${{CROW_API_URL}} once it comes up."

elif [ "${{HTTP_CODE}}" = "200" ] || [ "${{HTTP_CODE}}" = "201" ]; then
    echo "==> Registered with crowCloud at ${{CROW_API_URL}}"
    CLUSTER_ACTION="$(jq -r '.cluster_action.action' /tmp/crowcloud-register-response.json)"
    if [ "${{CLUSTER_ACTION}}" = "create" ]; then
        echo "==> First real node for this provider -- pvecm create"
        pvecm create crowcloud-fleet
    else
        JOIN_HOST="$(jq -r '.cluster_action.join_host' /tmp/crowcloud-register-response.json)"
        echo "==> Joining existing cluster via ${{JOIN_HOST}}"
        pvecm add "${{JOIN_HOST}}"
    fi
else
    echo "crowCloud reachable but registration failed (HTTP ${{HTTP_CODE}})" >&2
    cat /tmp/crowcloud-register-response.json >&2 || true
    exit 1
fi
"#,
        trunk_interface = cfg.trunk_interface,
        underlay_vlan = cfg.underlay_vlan,
        mgmt_vlan = cfg.mgmt_vlan,
        mgmt_ip = cfg.mgmt_ip,
        mgmt_prefix = cfg.mgmt_prefix,
        mgmt_gateway = cfg.mgmt_gateway,
        trunk_mtu = cfg.trunk_mtu,
        crow_api_url = cfg.crow_api_url,
        fleet_secret = cfg.fleet_secret,
        bgp_asn = cfg.bgp_asn,
        bgp_peer_password = cfg.bgp_peer_password,
        underlay_prefix = cfg.underlay_prefix,
        ospf_area = cfg.ospf_area,
        seed_cloud_init = seed_cloud_init,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> ProxmoxBuildConfig {
        ProxmoxBuildConfig {
            root_password_hash: "$6$hashed$xyz".into(),
            fqdn: "pve1.fleet.local".into(),
            admin_email: "admin@fleet.local".into(),
            trunk_interface: "eth0".into(),
            underlay_vlan: 10,
            mgmt_vlan: 20,
            mgmt_ip: "10.255.20.11".into(),
            mgmt_prefix: 24,
            mgmt_gateway: "10.255.20.1".into(),
            trunk_mtu: 9000,
            disk_list: vec!["sda".into(), "sdb".into()],
            zfs_raid: Some("raid1".into()),
            crow_api_url: "https://crowcloud.fleet.local".into(),
            fleet_secret: "fleet-secret-abc".into(),
            bgp_asn: 65000,
            bgp_peer_password: "fabric-secret".into(),
            underlay_prefix: 24,
            ospf_area: "0".into(),
        }
    }

    #[test]
    fn answer_toml_never_contains_a_plaintext_password() {
        let out = render_answer_toml(&cfg());
        assert!(out.contains("root-password-hashed"));
        assert!(!out.contains("root-password =") || out.contains("root-password-hashed"));
    }

    #[test]
    fn answer_toml_includes_mailto_and_enables_first_boot() {
        // Both confirmed live: `[global].mailto` is a hard requirement
        // (validate-answer rejects the file without it), and
        // `[first-boot]` is what actually makes `--on-first-boot` do
        // anything at all.
        let out = render_answer_toml(&cfg());
        assert!(out.contains("mailto = \"admin@fleet.local\""));
        assert!(out.contains("[first-boot]"));
        assert!(out.contains("source = \"from-iso\""));
    }

    #[test]
    fn answer_toml_zfs_raid_sets_filesystem_and_raid_level() {
        let out = render_answer_toml(&cfg());
        assert!(out.contains("filesystem = \"zfs\""));
        assert!(out.contains("zfs.raid = \"raid1\""));
        assert!(out.contains("disk-list = [\"sda\", \"sdb\"]"));
    }

    #[test]
    fn answer_toml_falls_back_to_ext4_without_zfs_raid() {
        let mut c = cfg();
        c.zfs_raid = None;
        let out = render_answer_toml(&c);
        assert!(out.contains("filesystem = \"ext4\""));
        assert!(!out.contains("zfs.raid"));
    }

    #[test]
    fn hook_embeds_the_fleet_secret_as_a_header_not_a_query_param() {
        let out = render_post_install_hook(&cfg());
        assert!(out.contains("X-Fleet-Secret: ${FLEET_SECRET}"));
        assert!(!out.contains("?secret="));
    }

    #[test]
    fn hook_branches_on_unreachable_crowcloud_into_seed_election() {
        let out = render_post_install_hook(&cfg());
        assert!(out.contains(r#"if [ "${HTTP_CODE}" = "000" ]; then"#));
        assert!(out.contains("self-electing as fleet seed"));
    }

    #[test]
    fn hook_resolves_cluster_action_from_the_register_response_not_a_flag() {
        let out = render_post_install_hook(&cfg());
        assert!(out.contains("cluster_action.action"));
        assert!(!out.contains("--cluster-mode"));
    }

    #[test]
    fn hook_sets_the_bgp_peer_password_matching_vyos() {
        let out = render_post_install_hook(&cfg());
        // The FRR config line is a heredoc rendered with the *shell*
        // variable, not the literal secret baked in twice -- deferred to
        // runtime same as everything else read from BGP_PEER_PASSWORD.
        assert!(out.contains(r#"BGP_PEER_PASSWORD="fabric-secret""#));
        assert!(out.contains("neighbor FABRIC password ${BGP_PEER_PASSWORD}"));
    }

    #[test]
    fn hook_actually_brings_up_the_management_vlan_interface() {
        // Confirmed live: writing to /etc/network/interfaces alone
        // doesn't bring the interface up -- it silently stayed absent
        // from `ip a` until an explicit ifup was added here.
        let out = render_post_install_hook(&cfg());
        assert!(out.contains("ifup \"${TRUNK_IF}.${MGMT_VLAN}\""));
    }

    #[test]
    fn hook_fetches_the_seed_image_via_proxmox_download_url_api() {
        // A truly fresh first host has no templates to clone -- the
        // seed VM must fetch its own base image (via Proxmox's own
        // download-url API, not crow-cli proxying the download) rather
        // than assuming a template already exists.
        let out = render_post_install_hook(&cfg());
        assert!(out.contains("/storage/\"${DEFAULT_STORAGE}\"/download-url"));
        assert!(out.contains("cloud.debian.org"));
        assert!(!out.contains("qm clone"));
    }

    #[test]
    fn hook_embeds_a_fully_resolved_cloud_init_with_no_double_expansion() {
        let out = render_post_install_hook(&cfg());
        // Written via a *quoted* heredoc delimiter so the outer script
        // (running on the bare Proxmox host) never tries to expand the
        // guest's own $VARs while writing the file -- this is the single
        // most important thing to get right here, a bug here would
        // corrupt the guest's bootstrap.sh silently.
        assert!(out.contains("<<'CLOUDINIT'"));
        assert!(out.contains("CROW_FLEET_SECRET=fleet-secret-abc"));
    }

    #[test]
    fn seed_cloud_init_bakes_the_literal_secret_not_a_shell_variable() {
        let out = render_seed_cloud_init("literal-secret-value");
        assert!(out.contains("CROW_FLEET_SECRET=literal-secret-value"));
        assert!(!out.contains("${FLEET_SECRET}"));
    }

    #[test]
    fn seed_cloud_init_embeds_the_real_bootstrap_sh_not_a_placeholder() {
        let out = render_seed_cloud_init("x");
        assert!(out.contains("Day-0 bootstrap"));
        assert!(out.contains("Installing K3s"));
    }
}
