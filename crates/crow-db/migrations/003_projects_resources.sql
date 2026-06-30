CREATE TABLE projects (
    id         UUID         PRIMARY KEY DEFAULT uuid_generate_v4(),
    name       VARCHAR(255) NOT NULL UNIQUE,
    created_by UUID         REFERENCES users(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

CREATE TABLE resource_groups (
    id         UUID         PRIMARY KEY DEFAULT uuid_generate_v4(),
    project    VARCHAR(255) NOT NULL REFERENCES projects(name) ON DELETE CASCADE,
    name       VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    UNIQUE (project, name)
);

CREATE TABLE resources (
    id             UUID         PRIMARY KEY DEFAULT uuid_generate_v4(),
    project        VARCHAR(255) NOT NULL,
    resource_group VARCHAR(255) NOT NULL,
    name           VARCHAR(255) NOT NULL,
    resource_type  VARCHAR(50)  NOT NULL,
    provider_id    UUID         REFERENCES providers(id) ON DELETE RESTRICT,
    spec           JSONB        NOT NULL,
    handle         JSONB,
    phase          VARCHAR(50)  NOT NULL DEFAULT 'Pending',
    created_by     UUID         REFERENCES users(id) ON DELETE SET NULL,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    UNIQUE (project, resource_group, name)
);

CREATE INDEX resources_project_rg_idx ON resources(project, resource_group);
CREATE INDEX resources_phase_idx       ON resources(phase);
