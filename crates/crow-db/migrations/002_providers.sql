CREATE TABLE providers (
    id            UUID        PRIMARY KEY DEFAULT uuid_generate_v4(),
    name          VARCHAR(255) NOT NULL UNIQUE,
    provider_type VARCHAR(50)  NOT NULL,
    config        JSONB        NOT NULL,
    created_by    UUID         REFERENCES users(id) ON DELETE SET NULL,
    created_at    TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);
