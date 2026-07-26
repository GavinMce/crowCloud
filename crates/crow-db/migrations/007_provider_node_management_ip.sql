-- Lets the self-registration callback (#65/#67) resolve a Proxmox cluster
-- join target dynamically -- "join whichever existing member most
-- recently registered" rather than a target baked into an image at
-- build time, which would go stale the moment that specific host is
-- down, rebuilt, or decommissioned. Nullable: existing rows created via
-- the manual node-adoption flow predate this and don't have one.
ALTER TABLE provider_nodes ADD COLUMN management_ip VARCHAR(45);
