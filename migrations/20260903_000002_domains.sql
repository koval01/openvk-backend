-- Full VK 2007 domain schema. Additive on top of 000001.
-- No visual-theme or microblog flags.

ALTER TABLE users ALTER COLUMN password_hash TYPE VARCHAR(255);
ALTER TABLE users ADD COLUMN IF NOT EXISTS password_algo VARCHAR(16) NOT NULL DEFAULT 'sha256';
ALTER TABLE users ADD COLUMN IF NOT EXISTS email_verified_at TIMESTAMPTZ;
ALTER TABLE users ADD COLUMN IF NOT EXISTS last_seen_at TIMESTAMPTZ;
ALTER TABLE users ADD COLUMN IF NOT EXISTS privacy_profile VARCHAR(16) NOT NULL DEFAULT 'everyone';
ALTER TABLE users ADD COLUMN IF NOT EXISTS privacy_photos VARCHAR(16) NOT NULL DEFAULT 'everyone';
ALTER TABLE users ADD COLUMN IF NOT EXISTS privacy_audio VARCHAR(16) NOT NULL DEFAULT 'everyone';

CREATE UNIQUE INDEX IF NOT EXISTS users_email_unique ON users (email) WHERE email IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS users_screen_name_unique ON users (screen_name) WHERE screen_name IS NOT NULL;

CREATE TABLE IF NOT EXISTS profiles (
    user_id BIGINT PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    hometown VARCHAR(60),
    birthday DATE,
    sex VARCHAR(16),
    relationship VARCHAR(32),
    university VARCHAR(120),
    school VARCHAR(120),
    activities TEXT,
    interests TEXT,
    favorite_music TEXT,
    favorite_films TEXT,
    favorite_books TEXT,
    favorite_quotes TEXT,
    about TEXT,
    website VARCHAR(255),
    address VARCHAR(120),
    contact_email VARCHAR(90),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS privacy_settings (
    user_id BIGINT PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    profile VARCHAR(16) NOT NULL DEFAULT 'everyone',
    photos VARCHAR(16) NOT NULL DEFAULT 'everyone',
    audio VARCHAR(16) NOT NULL DEFAULT 'everyone',
    wall VARCHAR(16) NOT NULL DEFAULT 'everyone',
    messages VARCHAR(16) NOT NULL DEFAULT 'everyone',
    friends_list VARCHAR(16) NOT NULL DEFAULT 'everyone'
);

CREATE TABLE IF NOT EXISTS refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash CHAR(64) NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    user_agent VARCHAR(255),
    ip VARCHAR(45),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS refresh_tokens_user ON refresh_tokens (user_id);

CREATE TABLE IF NOT EXISTS email_verification_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash CHAR(64) NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS password_reset_tokens (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    token_hash CHAR(64) NOT NULL UNIQUE,
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS email_outbox (
    id BIGSERIAL PRIMARY KEY,
    to_email VARCHAR(90) NOT NULL,
    template VARCHAR(64) NOT NULL,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    status VARCHAR(16) NOT NULL DEFAULT 'queued',
    attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    scheduled_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    sent_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS email_outbox_status_scheduled
    ON email_outbox (status, scheduled_at);

ALTER TABLE friendships ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

CREATE TABLE IF NOT EXISTS friend_requests (
    id BIGSERIAL PRIMARY KEY,
    from_user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    to_user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    status VARCHAR(16) NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    responded_at TIMESTAMPTZ,
    UNIQUE (from_user_id, to_user_id),
    CHECK (from_user_id <> to_user_id)
);

CREATE INDEX IF NOT EXISTS friend_requests_to_status ON friend_requests (to_user_id, status);
CREATE INDEX IF NOT EXISTS friend_requests_from_status ON friend_requests (from_user_id, status);

CREATE TABLE IF NOT EXISTS follows (
    follower_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    target_user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (follower_id, target_user_id),
    CHECK (follower_id <> target_user_id)
);

ALTER TABLE wall_posts ADD COLUMN IF NOT EXISTS group_id BIGINT;
ALTER TABLE wall_posts ADD COLUMN IF NOT EXISTS deleted_at TIMESTAMPTZ;

CREATE TABLE IF NOT EXISTS groups (
    id BIGSERIAL PRIMARY KEY,
    slug VARCHAR(36) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    about TEXT,
    kind VARCHAR(16) NOT NULL DEFAULT 'open',
    owner_id BIGINT NOT NULL REFERENCES users (id) ON DELETE RESTRICT,
    avatar_key VARCHAR(512),
    wall_open BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'wall_posts_group_id_fkey'
    ) THEN
        ALTER TABLE wall_posts
            ADD CONSTRAINT wall_posts_group_id_fkey
            FOREIGN KEY (group_id) REFERENCES groups (id) ON DELETE CASCADE;
    END IF;
END $$;

CREATE TABLE IF NOT EXISTS group_members (
    group_id BIGINT NOT NULL REFERENCES groups (id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role VARCHAR(16) NOT NULL DEFAULT 'member',
    status VARCHAR(16) NOT NULL DEFAULT 'active',
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (group_id, user_id)
);

CREATE INDEX IF NOT EXISTS group_members_user ON group_members (user_id, status);

CREATE TABLE IF NOT EXISTS group_topics (
    id BIGSERIAL PRIMARY KEY,
    group_id BIGINT NOT NULL REFERENCES groups (id) ON DELETE CASCADE,
    author_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title VARCHAR(128) NOT NULL,
    closed BOOLEAN NOT NULL DEFAULT FALSE,
    pinned BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS group_topic_posts (
    id BIGSERIAL PRIMARY KEY,
    topic_id BIGINT NOT NULL REFERENCES group_topics (id) ON DELETE CASCADE,
    author_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS conversations (
    id BIGSERIAL PRIMARY KEY,
    kind VARCHAR(16) NOT NULL DEFAULT 'direct',
    title VARCHAR(128),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS conversation_members (
    conversation_id BIGINT NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role VARCHAR(16) NOT NULL DEFAULT 'member',
    last_read_message_id BIGINT,
    typing_at TIMESTAMPTZ,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (conversation_id, user_id)
);

CREATE TABLE IF NOT EXISTS messages (
    id BIGSERIAL PRIMARY KEY,
    conversation_id BIGINT NOT NULL REFERENCES conversations (id) ON DELETE CASCADE,
    author_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    edited_at TIMESTAMPTZ,
    deleted_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS messages_conversation_created
    ON messages (conversation_id, created_at DESC);

CREATE TABLE IF NOT EXISTS message_receipts (
    message_id BIGINT NOT NULL REFERENCES messages (id) ON DELETE CASCADE,
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    delivered_at TIMESTAMPTZ,
    read_at TIMESTAMPTZ,
    PRIMARY KEY (message_id, user_id)
);

CREATE TABLE IF NOT EXISTS media_objects (
    id BIGSERIAL PRIMARY KEY,
    owner_user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind VARCHAR(16) NOT NULL,
    storage_key VARCHAR(512) NOT NULL,
    mime VARCHAR(127) NOT NULL,
    size_bytes BIGINT NOT NULL DEFAULT 0,
    width INTEGER,
    height INTEGER,
    duration_ms INTEGER,
    original_filename VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS media_objects_owner_kind ON media_objects (owner_user_id, kind);

CREATE TABLE IF NOT EXISTS albums (
    id BIGSERIAL PRIMARY KEY,
    owner_user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title VARCHAR(64) NOT NULL,
    description TEXT,
    cover_media_id BIGINT REFERENCES media_objects (id) ON DELETE SET NULL,
    privacy VARCHAR(16) NOT NULL DEFAULT 'everyone',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS album_photos (
    album_id BIGINT NOT NULL REFERENCES albums (id) ON DELETE CASCADE,
    media_id BIGINT NOT NULL REFERENCES media_objects (id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (album_id, media_id)
);

CREATE TABLE IF NOT EXISTS photo_tags (
    id BIGSERIAL PRIMARY KEY,
    media_id BIGINT NOT NULL REFERENCES media_objects (id) ON DELETE CASCADE,
    tagged_user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    tagged_by_user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    x_percent REAL NOT NULL DEFAULT 50,
    y_percent REAL NOT NULL DEFAULT 50,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS audios (
    id BIGSERIAL PRIMARY KEY,
    media_id BIGINT NOT NULL REFERENCES media_objects (id) ON DELETE CASCADE,
    owner_user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    artist VARCHAR(256) NOT NULL,
    title VARCHAR(256) NOT NULL,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    genre VARCHAR(64),
    lyrics TEXT,
    listens BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS audios_owner ON audios (owner_user_id);
CREATE INDEX IF NOT EXISTS audios_search ON audios (artist, title);

CREATE TABLE IF NOT EXISTS audio_library (
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    audio_id BIGINT NOT NULL REFERENCES audios (id) ON DELETE CASCADE,
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (user_id, audio_id)
);

CREATE TABLE IF NOT EXISTS playlists (
    id BIGSERIAL PRIMARY KEY,
    owner_user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title VARCHAR(128) NOT NULL,
    description VARCHAR(2048),
    cover_media_id BIGINT REFERENCES media_objects (id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS playlist_tracks (
    playlist_id BIGINT NOT NULL REFERENCES playlists (id) ON DELETE CASCADE,
    audio_id BIGINT NOT NULL REFERENCES audios (id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (playlist_id, audio_id)
);

CREATE TABLE IF NOT EXISTS videos (
    id BIGSERIAL PRIMARY KEY,
    media_id BIGINT REFERENCES media_objects (id) ON DELETE SET NULL,
    owner_user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title VARCHAR(128) NOT NULL,
    description TEXT,
    status VARCHAR(16) NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS notifications (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    kind VARCHAR(32) NOT NULL,
    actor_id BIGINT REFERENCES users (id) ON DELETE SET NULL,
    entity_type VARCHAR(32),
    entity_id BIGINT,
    payload JSONB NOT NULL DEFAULT '{}'::jsonb,
    read_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS notifications_user_created
    ON notifications (user_id, created_at DESC);

CREATE TABLE IF NOT EXISTS feed_events (
    id BIGSERIAL PRIMARY KEY,
    actor_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    verb VARCHAR(32) NOT NULL,
    object_type VARCHAR(32) NOT NULL,
    object_id BIGINT NOT NULL,
    group_id BIGINT REFERENCES groups (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS feed_events_created ON feed_events (created_at DESC);
CREATE INDEX IF NOT EXISTS feed_events_actor_created ON feed_events (actor_id, created_at DESC);

INSERT INTO privacy_settings (user_id, profile, photos, audio, wall, messages, friends_list)
SELECT id, privacy_profile, privacy_photos, privacy_audio, privacy_wall, privacy_messages, 'everyone'
FROM users
ON CONFLICT (user_id) DO NOTHING;

INSERT INTO profiles (user_id)
SELECT id FROM users
ON CONFLICT (user_id) DO NOTHING;
