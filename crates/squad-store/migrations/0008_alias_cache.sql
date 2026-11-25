-- Additive only: preserve all carrier tables, indexes and foreign keys.
CREATE TABLE alias_cache (
    provider TEXT NOT NULL,
    alias TEXT NOT NULL CHECK(substr(alias,1,1)='~'),
    model_json TEXT NOT NULL CHECK(json_valid(model_json)),
    resolved_at INTEGER NOT NULL CHECK(resolved_at >= 0),
    PRIMARY KEY(provider, alias)
) STRICT;
