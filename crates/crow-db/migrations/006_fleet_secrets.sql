-- Shared secret(s) baked into every image built by crow-cli's ISO builder
-- (#66) at build time -- not fetched or minted per-host at install time.
-- A host presenting a valid, non-revoked secret via X-Fleet-Secret can
-- self-register with zero admin action per host. Trust is anchored to
-- "built by our own tooling", not "merely reachable on the network".
--
-- Supports rotation: multiple rows can be valid at once, letting an older
-- secret stay accepted until every image still using it has been
-- replaced, then revoked.
CREATE TABLE fleet_secrets (
    id         UUID         PRIMARY KEY DEFAULT uuid_generate_v4(),
    secret     VARCHAR(64)  NOT NULL UNIQUE,
    label      VARCHAR(255),
    created_by UUID         REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    revoked_at TIMESTAMPTZ
);
