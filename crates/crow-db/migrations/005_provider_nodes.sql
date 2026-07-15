-- Per-node config (default storage/bridge) for a provider's discovered
-- nodes. A node Proxmox reports with no row here is "discovered but not
-- configured" — the API falls back to providers.config for the legacy
-- primary node when no row exists, so this table only needs a row once a
-- node's defaults are actually set (including re-set) via the API.
CREATE TABLE provider_nodes (
    id              UUID         PRIMARY KEY DEFAULT uuid_generate_v4(),
    provider_id     UUID         NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    node_name       VARCHAR(255) NOT NULL,
    default_storage VARCHAR(255) NOT NULL,
    default_bridge  VARCHAR(255) NOT NULL,
    created_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    UNIQUE (provider_id, node_name)
);
