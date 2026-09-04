-- Core identity + classic guestbook wall + accepted friend edges.
-- Theme / microblog / AJAX / infinite-scroll columns are intentionally absent.

CREATE TABLE IF NOT EXISTS users (
    id BIGSERIAL PRIMARY KEY,
    login VARCHAR(64) NOT NULL UNIQUE,
    password_hash VARCHAR(255) NOT NULL,
    first_name VARCHAR(50) NOT NULL,
    last_name VARCHAR(50) NOT NULL,
    screen_name VARCHAR(36),
    status VARCHAR(255),
    city VARCHAR(60),
    email VARCHAR(90),
    phone VARCHAR(36),
    avatar_url VARCHAR(512),
    verified BOOLEAN NOT NULL DEFAULT FALSE,
    privacy_wall VARCHAR(16) NOT NULL DEFAULT 'everyone',
    privacy_messages VARCHAR(16) NOT NULL DEFAULT 'everyone',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS wall_posts (
    id BIGSERIAL PRIMARY KEY,
    target_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    author_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS wall_posts_target_created
    ON wall_posts (target_id, created_at DESC);

CREATE TABLE IF NOT EXISTS friendships (
    user_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    friend_id BIGINT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, friend_id)
);
