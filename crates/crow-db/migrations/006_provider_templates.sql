-- Per-VMID OS metadata (family/distribution/version/display name) for a
-- provider's discovered Proxmox templates, mirroring provider_nodes: a
-- template Proxmox reports with no row here is "discovered but not tagged"
-- and won't appear as a preset option in VM creation.
--
-- Keyed on (provider_id, vmid) rather than (provider_id, node_name, vmid) —
-- Proxmox VMIDs are unique cluster-wide, so node_name is informational
-- (which node the template currently lives on), not a disambiguator.
CREATE TABLE provider_templates (
    id              UUID         PRIMARY KEY DEFAULT uuid_generate_v4(),
    provider_id     UUID         NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    node_name       VARCHAR(255) NOT NULL,
    vmid            INTEGER      NOT NULL,
    display_name    VARCHAR(255) NOT NULL,
    os_family       VARCHAR(20)  NOT NULL,
    distribution    VARCHAR(100) NOT NULL,
    version         VARCHAR(50)  NOT NULL,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    UNIQUE (provider_id, vmid)
);
