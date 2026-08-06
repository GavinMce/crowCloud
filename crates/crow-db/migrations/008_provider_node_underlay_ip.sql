-- Lets `create_network` (Proxmox VXLAN/EVPN dataplane) resolve a node's own
-- underlay-VLAN IP as its VTEP source address, without needing a separate
-- lookup or config field -- mirrors 007's management_ip addition exactly.
-- Nullable: rows created before this existed (or via the manual
-- node-adoption flow) don't have one yet.
ALTER TABLE provider_nodes ADD COLUMN underlay_ip VARCHAR(45);
