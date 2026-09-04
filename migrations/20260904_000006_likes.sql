-- OpenVK likes: origin + (kind, owner, object). Wall object_id is the local
-- wall id (wall{owner}_{local}); photos/videos/comments use their public ids.

CREATE TABLE IF NOT EXISTS likes (
    origin BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    target_kind TEXT NOT NULL,
    owner_id BIGINT NOT NULL,
    object_id BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (origin, target_kind, owner_id, object_id)
);

CREATE INDEX IF NOT EXISTS likes_target_idx
    ON likes (target_kind, owner_id, object_id);
