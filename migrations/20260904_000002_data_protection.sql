-- Application-level encryption at rest.
-- Ciphertexts are TEXT (`ovk1.` + base64). Lookup uses HMAC blind indexes.

ALTER TABLE users ALTER COLUMN email TYPE TEXT;
ALTER TABLE users ALTER COLUMN phone TYPE TEXT;
ALTER TABLE users ALTER COLUMN city TYPE TEXT;

DROP INDEX IF EXISTS users_email_unique;

ALTER TABLE users ADD COLUMN IF NOT EXISTS email_idx VARCHAR(64);
ALTER TABLE users ADD COLUMN IF NOT EXISTS phone_idx VARCHAR(64);
ALTER TABLE users ADD COLUMN IF NOT EXISTS wrap_key TEXT;

CREATE UNIQUE INDEX IF NOT EXISTS users_email_idx_unique
    ON users (email_idx) WHERE email_idx IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS users_phone_idx_unique
    ON users (phone_idx) WHERE phone_idx IS NOT NULL;

ALTER TABLE profiles ALTER COLUMN hometown TYPE TEXT;
ALTER TABLE profiles ALTER COLUMN address TYPE TEXT;
ALTER TABLE profiles ALTER COLUMN contact_email TYPE TEXT;
ALTER TABLE profiles
    ALTER COLUMN birthday TYPE TEXT USING birthday::text;

CREATE TABLE IF NOT EXISTS conversation_keys (
    conversation_id BIGINT NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    wrapped_dek TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (conversation_id, user_id)
);
