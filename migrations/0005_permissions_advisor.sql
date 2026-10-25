-- Per-user permission sets and the local advisor's connection settings.

-- NULL means "the role's default set"; an array replaces it with an explicit list of
-- permission names (see yusr_server::auth::Permission). Admins always keep every permission.
ALTER TABLE users ADD COLUMN permissions TEXT[];

-- {"url": "...", "model": "...", "enabled": true}; empty means the server's defaults.
ALTER TABLE settings ADD COLUMN advisor JSONB NOT NULL DEFAULT '{}'::jsonb;
