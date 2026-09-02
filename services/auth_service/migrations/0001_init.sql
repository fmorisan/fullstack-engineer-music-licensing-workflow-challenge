-- auth_service initial schema (ADR-003).

-- Organizations: studios and labels that users belong to.
CREATE TABLE organizations (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('STUDIO', 'LABEL')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT organizations_name_kind_unique UNIQUE (name, kind)
);

-- Users.
-- role/org_id consistency is enforced in-database: admins belong to no
-- organization, studio/label users must belong to one.
CREATE TABLE users (
    id UUID PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    display_name TEXT NOT NULL,
    role TEXT NOT NULL CHECK (role IN ('STUDIO', 'LABEL', 'ADMIN')),
    org_id UUID REFERENCES organizations (id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT users_role_org_consistency CHECK (
        (role = 'ADMIN' AND org_id IS NULL)
        OR (role <> 'ADMIN' AND org_id IS NOT NULL)
    )
);

CREATE INDEX users_org_id_idx ON users (org_id);
