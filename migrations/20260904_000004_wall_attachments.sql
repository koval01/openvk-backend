-- Club walls use a negative target_id (VK owner id). Drop the users FK so
-- that value can live in wall_posts.target_id. Attachments, geo, source, NSFW
-- and comments belong to the wall family (permalink /wall{owner}_{local}).

ALTER TABLE wall_posts DROP CONSTRAINT IF EXISTS wall_posts_target_id_fkey;

ALTER TABLE wall_posts ADD COLUMN IF NOT EXISTS geo_lat DOUBLE PRECISION;
ALTER TABLE wall_posts ADD COLUMN IF NOT EXISTS geo_lng DOUBLE PRECISION;
ALTER TABLE wall_posts ADD COLUMN IF NOT EXISTS geo_name TEXT;
ALTER TABLE wall_posts ADD COLUMN IF NOT EXISTS source TEXT;
ALTER TABLE wall_posts ADD COLUMN IF NOT EXISTS nsfw BOOLEAN NOT NULL DEFAULT FALSE;

ALTER TABLE groups ADD COLUMN IF NOT EXISTS wall_seq BIGINT NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS wall_attachments (
    id BIGSERIAL PRIMARY KEY,
    post_id BIGINT NOT NULL REFERENCES wall_posts (id) ON DELETE CASCADE,
    sort INTEGER NOT NULL DEFAULT 0,
    kind TEXT NOT NULL,
    owner_id BIGINT NOT NULL DEFAULT 0,
    object_id BIGINT NOT NULL DEFAULT 0,
    url TEXT NOT NULL DEFAULT '',
    title TEXT NOT NULL DEFAULT '',
    src TEXT NOT NULL DEFAULT ''
);

CREATE INDEX IF NOT EXISTS wall_attachments_post_id_idx
    ON wall_attachments (post_id, sort);

CREATE TABLE IF NOT EXISTS comments (
    id BIGINT PRIMARY KEY,
    target_kind TEXT NOT NULL,
    owner_id BIGINT NOT NULL,
    object_id BIGINT NOT NULL,
    author_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS comments_target_idx
    ON comments (target_kind, owner_id, object_id, created_at)
    WHERE deleted_at IS NULL;
