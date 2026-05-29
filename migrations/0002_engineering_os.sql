CREATE TABLE IF NOT EXISTS memory_nodes (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    label TEXT NOT NULL,
    summary TEXT NOT NULL DEFAULT '',
    confidence SMALLINT NOT NULL DEFAULT 0,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS memory_nodes_project_kind_idx ON memory_nodes (project_id, kind);
CREATE INDEX IF NOT EXISTS memory_nodes_label_idx ON memory_nodes USING gin (to_tsvector('simple', label || ' ' || summary));

CREATE TABLE IF NOT EXISTS memory_edges (
    id BIGSERIAL PRIMARY KEY,
    project_id TEXT NOT NULL,
    from_node TEXT NOT NULL REFERENCES memory_nodes(id) ON DELETE CASCADE,
    relation TEXT NOT NULL,
    to_node TEXT NOT NULL REFERENCES memory_nodes(id) ON DELETE CASCADE,
    weight SMALLINT NOT NULL DEFAULT 50,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (project_id, from_node, relation, to_node)
);

CREATE INDEX IF NOT EXISTS memory_edges_project_relation_idx ON memory_edges (project_id, relation);

CREATE TABLE IF NOT EXISTS memory_embeddings (
    node_id TEXT PRIMARY KEY REFERENCES memory_nodes(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    vector_id TEXT NOT NULL,
    dimensions INTEGER NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS repositories (
    id TEXT PRIMARY KEY,
    root TEXT NOT NULL,
    head_sha TEXT NOT NULL DEFAULT '',
    language TEXT NOT NULL DEFAULT 'unknown',
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb,
    indexed_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS repo_symbols (
    id BIGSERIAL PRIMARY KEY,
    repo_id TEXT NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    span JSONB NOT NULL DEFAULT '{}'::jsonb,
    metadata JSONB NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS repo_symbols_repo_path_idx ON repo_symbols (repo_id, path);
CREATE INDEX IF NOT EXISTS repo_symbols_name_idx ON repo_symbols (name);

CREATE TABLE IF NOT EXISTS code_tasks (
    id TEXT PRIMARY KEY,
    repo_id TEXT REFERENCES repositories(id) ON DELETE SET NULL,
    objective TEXT NOT NULL,
    plan JSONB NOT NULL DEFAULT '{}'::jsonb,
    status TEXT NOT NULL DEFAULT 'planned',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS patch_sets (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES code_tasks(id) ON DELETE CASCADE,
    diff TEXT NOT NULL,
    validation JSONB NOT NULL DEFAULT '{}'::jsonb,
    review JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS consensus_votes (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    agent TEXT NOT NULL,
    decision TEXT NOT NULL,
    rationale TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS infra_plans (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    objective TEXT NOT NULL,
    plan JSONB NOT NULL,
    risk_score SMALLINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS generated_artifacts (
    id TEXT PRIMARY KEY,
    plan_id TEXT REFERENCES infra_plans(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    path TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS risk_scores (
    id TEXT PRIMARY KEY,
    service TEXT NOT NULL,
    score SMALLINT NOT NULL,
    horizon TEXT NOT NULL,
    evidence JSONB NOT NULL DEFAULT '[]'::jsonb,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS remediation_policies (
    id TEXT PRIMARY KEY,
    selector JSONB NOT NULL,
    action JSONB NOT NULL,
    risk TEXT NOT NULL,
    approvals JSONB NOT NULL DEFAULT '[]'::jsonb,
    enabled BOOLEAN NOT NULL DEFAULT false
);

CREATE TABLE IF NOT EXISTS security_findings (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    severity TEXT NOT NULL,
    title TEXT NOT NULL,
    evidence TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS model_routing_events (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    model TEXT NOT NULL,
    provider TEXT NOT NULL,
    latency_ms BIGINT,
    quality_score SMALLINT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS replay_sessions (
    id TEXT PRIMARY KEY,
    incident_id TEXT,
    cursor BIGINT NOT NULL DEFAULT 0,
    mode TEXT NOT NULL DEFAULT 'analysis',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
