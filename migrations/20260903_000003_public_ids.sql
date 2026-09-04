-- Public objects use application-assigned random 32-bit ids.
-- Wall posts keep a sequential local_id scoped to the profile they belong to.

ALTER TABLE users ADD COLUMN IF NOT EXISTS wall_seq BIGINT NOT NULL DEFAULT 0;

ALTER TABLE wall_posts ADD COLUMN IF NOT EXISTS local_id BIGINT;

UPDATE wall_posts
SET local_id = numbered.n
FROM (
    SELECT id, ROW_NUMBER() OVER (PARTITION BY target_id ORDER BY created_at, id) AS n
    FROM wall_posts
) AS numbered
WHERE wall_posts.id = numbered.id;

UPDATE wall_posts SET local_id = 1 WHERE local_id IS NULL;

ALTER TABLE wall_posts ALTER COLUMN local_id SET NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS wall_posts_target_local
    ON wall_posts (target_id, local_id);

UPDATE users
SET wall_seq = COALESCE((
    SELECT MAX(local_id) FROM wall_posts WHERE wall_posts.target_id = users.id
), 0);

ALTER TABLE users ALTER COLUMN id DROP DEFAULT;
ALTER TABLE groups ALTER COLUMN id DROP DEFAULT;
ALTER TABLE media_objects ALTER COLUMN id DROP DEFAULT;
ALTER TABLE albums ALTER COLUMN id DROP DEFAULT;
ALTER TABLE audios ALTER COLUMN id DROP DEFAULT;
ALTER TABLE videos ALTER COLUMN id DROP DEFAULT;
ALTER TABLE playlists ALTER COLUMN id DROP DEFAULT;
ALTER TABLE conversations ALTER COLUMN id DROP DEFAULT;

DROP SEQUENCE IF EXISTS users_id_seq;
DROP SEQUENCE IF EXISTS groups_id_seq;
DROP SEQUENCE IF EXISTS media_objects_id_seq;
DROP SEQUENCE IF EXISTS albums_id_seq;
DROP SEQUENCE IF EXISTS audios_id_seq;
DROP SEQUENCE IF EXISTS videos_id_seq;
DROP SEQUENCE IF EXISTS playlists_id_seq;
DROP SEQUENCE IF EXISTS conversations_id_seq;

-- Remint existing public ids so demo rows are not 1, 2, 3.
DO $remint$
DECLARE
    rec RECORD;
    candidate BIGINT;
    shift CONSTANT BIGINT := 4000000000;
BEGIN
    CREATE TEMP TABLE saved_fks (
        tbl TEXT NOT NULL,
        conname TEXT NOT NULL,
        def TEXT NOT NULL
    ) ON COMMIT DROP;

    INSERT INTO saved_fks (tbl, conname, def)
    SELECT conrelid::regclass::text, conname, pg_get_constraintdef(oid)
    FROM pg_constraint
    WHERE contype = 'f'
      AND confrelid IN (
          'users'::regclass,
          'groups'::regclass,
          'media_objects'::regclass,
          'albums'::regclass,
          'audios'::regclass,
          'videos'::regclass
      );

    FOR rec IN SELECT * FROM saved_fks LOOP
        EXECUTE format('ALTER TABLE %s DROP CONSTRAINT %I', rec.tbl, rec.conname);
    END LOOP;

    UPDATE users SET id = id + shift;
    UPDATE wall_posts SET target_id = target_id + shift, author_id = author_id + shift;
    UPDATE friendships SET user_id = user_id + shift, friend_id = friend_id + shift;
    UPDATE profiles SET user_id = user_id + shift;
    UPDATE privacy_settings SET user_id = user_id + shift;
    UPDATE refresh_tokens SET user_id = user_id + shift;
    UPDATE email_verification_tokens SET user_id = user_id + shift;
    UPDATE password_reset_tokens SET user_id = user_id + shift;
    UPDATE friend_requests SET from_user_id = from_user_id + shift, to_user_id = to_user_id + shift;
    UPDATE follows SET follower_id = follower_id + shift, target_user_id = target_user_id + shift;
    UPDATE groups SET owner_id = owner_id + shift;
    UPDATE group_members SET user_id = user_id + shift;
    UPDATE group_topics SET author_id = author_id + shift;
    UPDATE group_topic_posts SET author_id = author_id + shift;
    UPDATE conversation_members SET user_id = user_id + shift;
    UPDATE messages SET author_id = author_id + shift;
    UPDATE message_receipts SET user_id = user_id + shift;
    UPDATE media_objects SET owner_user_id = owner_user_id + shift;
    UPDATE albums SET owner_user_id = owner_user_id + shift;
    UPDATE photo_tags
        SET tagged_user_id = tagged_user_id + shift,
            tagged_by_user_id = tagged_by_user_id + shift;
    UPDATE audios SET owner_user_id = owner_user_id + shift;
    UPDATE audio_library SET user_id = user_id + shift;
    UPDATE playlists SET owner_user_id = owner_user_id + shift;
    UPDATE videos SET owner_user_id = owner_user_id + shift;
    UPDATE notifications SET user_id = user_id + shift;
    UPDATE notifications SET actor_id = actor_id + shift WHERE actor_id IS NOT NULL;
    UPDATE feed_events SET actor_id = actor_id + shift;

    CREATE TEMP TABLE user_map (old_id BIGINT PRIMARY KEY, new_id BIGINT UNIQUE) ON COMMIT DROP;
    FOR rec IN SELECT id FROM users LOOP
        LOOP
            candidate := 1 + floor(random() * 2147483646)::BIGINT;
            EXIT WHEN candidate > 0
                AND NOT EXISTS (SELECT 1 FROM user_map WHERE new_id = candidate);
        END LOOP;
        INSERT INTO user_map (old_id, new_id) VALUES (rec.id, candidate);
    END LOOP;

    UPDATE users u SET id = m.new_id FROM user_map m WHERE u.id = m.old_id;
    UPDATE wall_posts t SET target_id = m.new_id FROM user_map m WHERE t.target_id = m.old_id;
    UPDATE wall_posts t SET author_id = m.new_id FROM user_map m WHERE t.author_id = m.old_id;
    UPDATE friendships t SET user_id = m.new_id FROM user_map m WHERE t.user_id = m.old_id;
    UPDATE friendships t SET friend_id = m.new_id FROM user_map m WHERE t.friend_id = m.old_id;
    UPDATE profiles t SET user_id = m.new_id FROM user_map m WHERE t.user_id = m.old_id;
    UPDATE privacy_settings t SET user_id = m.new_id FROM user_map m WHERE t.user_id = m.old_id;
    UPDATE refresh_tokens t SET user_id = m.new_id FROM user_map m WHERE t.user_id = m.old_id;
    UPDATE email_verification_tokens t SET user_id = m.new_id FROM user_map m WHERE t.user_id = m.old_id;
    UPDATE password_reset_tokens t SET user_id = m.new_id FROM user_map m WHERE t.user_id = m.old_id;
    UPDATE friend_requests t SET from_user_id = m.new_id FROM user_map m WHERE t.from_user_id = m.old_id;
    UPDATE friend_requests t SET to_user_id = m.new_id FROM user_map m WHERE t.to_user_id = m.old_id;
    UPDATE follows t SET follower_id = m.new_id FROM user_map m WHERE t.follower_id = m.old_id;
    UPDATE follows t SET target_user_id = m.new_id FROM user_map m WHERE t.target_user_id = m.old_id;
    UPDATE groups t SET owner_id = m.new_id FROM user_map m WHERE t.owner_id = m.old_id;
    UPDATE group_members t SET user_id = m.new_id FROM user_map m WHERE t.user_id = m.old_id;
    UPDATE group_topics t SET author_id = m.new_id FROM user_map m WHERE t.author_id = m.old_id;
    UPDATE group_topic_posts t SET author_id = m.new_id FROM user_map m WHERE t.author_id = m.old_id;
    UPDATE conversation_members t SET user_id = m.new_id FROM user_map m WHERE t.user_id = m.old_id;
    UPDATE messages t SET author_id = m.new_id FROM user_map m WHERE t.author_id = m.old_id;
    UPDATE message_receipts t SET user_id = m.new_id FROM user_map m WHERE t.user_id = m.old_id;
    UPDATE media_objects t SET owner_user_id = m.new_id FROM user_map m WHERE t.owner_user_id = m.old_id;
    UPDATE albums t SET owner_user_id = m.new_id FROM user_map m WHERE t.owner_user_id = m.old_id;
    UPDATE photo_tags t SET tagged_user_id = m.new_id FROM user_map m WHERE t.tagged_user_id = m.old_id;
    UPDATE photo_tags t SET tagged_by_user_id = m.new_id FROM user_map m WHERE t.tagged_by_user_id = m.old_id;
    UPDATE audios t SET owner_user_id = m.new_id FROM user_map m WHERE t.owner_user_id = m.old_id;
    UPDATE audio_library t SET user_id = m.new_id FROM user_map m WHERE t.user_id = m.old_id;
    UPDATE playlists t SET owner_user_id = m.new_id FROM user_map m WHERE t.owner_user_id = m.old_id;
    UPDATE videos t SET owner_user_id = m.new_id FROM user_map m WHERE t.owner_user_id = m.old_id;
    UPDATE notifications t SET user_id = m.new_id FROM user_map m WHERE t.user_id = m.old_id;
    UPDATE notifications t SET actor_id = m.new_id FROM user_map m WHERE t.actor_id = m.old_id;
    UPDATE feed_events t SET actor_id = m.new_id FROM user_map m WHERE t.actor_id = m.old_id;

    UPDATE groups SET id = id + shift;
    UPDATE wall_posts SET group_id = group_id + shift WHERE group_id IS NOT NULL;
    UPDATE group_members SET group_id = group_id + shift;
    UPDATE group_topics SET group_id = group_id + shift;
    UPDATE feed_events SET group_id = group_id + shift WHERE group_id IS NOT NULL;

    CREATE TEMP TABLE group_map (old_id BIGINT PRIMARY KEY, new_id BIGINT UNIQUE) ON COMMIT DROP;
    FOR rec IN SELECT id FROM groups LOOP
        LOOP
            candidate := 1 + floor(random() * 2147483646)::BIGINT;
            EXIT WHEN NOT EXISTS (SELECT 1 FROM group_map WHERE new_id = candidate);
        END LOOP;
        INSERT INTO group_map (old_id, new_id) VALUES (rec.id, candidate);
    END LOOP;

    UPDATE groups t SET id = m.new_id FROM group_map m WHERE t.id = m.old_id;
    UPDATE wall_posts t SET group_id = m.new_id FROM group_map m WHERE t.group_id = m.old_id;
    UPDATE group_members t SET group_id = m.new_id FROM group_map m WHERE t.group_id = m.old_id;
    UPDATE group_topics t SET group_id = m.new_id FROM group_map m WHERE t.group_id = m.old_id;
    UPDATE feed_events t SET group_id = m.new_id FROM group_map m WHERE t.group_id = m.old_id;

    UPDATE media_objects SET id = id + shift;
    UPDATE albums SET cover_media_id = cover_media_id + shift WHERE cover_media_id IS NOT NULL;
    UPDATE album_photos SET media_id = media_id + shift;
    UPDATE photo_tags SET media_id = media_id + shift;
    UPDATE audios SET media_id = media_id + shift;
    UPDATE playlists SET cover_media_id = cover_media_id + shift WHERE cover_media_id IS NOT NULL;
    UPDATE videos SET media_id = media_id + shift WHERE media_id IS NOT NULL;

    CREATE TEMP TABLE media_map (old_id BIGINT PRIMARY KEY, new_id BIGINT UNIQUE) ON COMMIT DROP;
    FOR rec IN SELECT id FROM media_objects LOOP
        LOOP
            candidate := 1 + floor(random() * 2147483646)::BIGINT;
            EXIT WHEN NOT EXISTS (SELECT 1 FROM media_map WHERE new_id = candidate);
        END LOOP;
        INSERT INTO media_map (old_id, new_id) VALUES (rec.id, candidate);
    END LOOP;

    UPDATE media_objects t SET id = m.new_id FROM media_map m WHERE t.id = m.old_id;
    UPDATE albums t SET cover_media_id = m.new_id FROM media_map m WHERE t.cover_media_id = m.old_id;
    UPDATE album_photos t SET media_id = m.new_id FROM media_map m WHERE t.media_id = m.old_id;
    UPDATE photo_tags t SET media_id = m.new_id FROM media_map m WHERE t.media_id = m.old_id;
    UPDATE audios t SET media_id = m.new_id FROM media_map m WHERE t.media_id = m.old_id;
    UPDATE playlists t SET cover_media_id = m.new_id FROM media_map m WHERE t.cover_media_id = m.old_id;
    UPDATE videos t SET media_id = m.new_id FROM media_map m WHERE t.media_id = m.old_id;
    UPDATE users t
        SET avatar_url = replace(
            t.avatar_url,
            '/api/v1/media/' || (m.old_id - shift) || '/content',
            '/api/v1/media/' || m.new_id || '/content'
        )
    FROM media_map m
    WHERE t.avatar_url LIKE '%/api/v1/media/' || (m.old_id - shift) || '/content%';

    UPDATE albums SET id = id + shift;
    UPDATE album_photos SET album_id = album_id + shift;

    CREATE TEMP TABLE album_map (old_id BIGINT PRIMARY KEY, new_id BIGINT UNIQUE) ON COMMIT DROP;
    FOR rec IN SELECT id FROM albums LOOP
        LOOP
            candidate := 1 + floor(random() * 2147483646)::BIGINT;
            EXIT WHEN NOT EXISTS (SELECT 1 FROM album_map WHERE new_id = candidate);
        END LOOP;
        INSERT INTO album_map (old_id, new_id) VALUES (rec.id, candidate);
    END LOOP;

    UPDATE albums t SET id = m.new_id FROM album_map m WHERE t.id = m.old_id;
    UPDATE album_photos t SET album_id = m.new_id FROM album_map m WHERE t.album_id = m.old_id;

    UPDATE audios SET id = id + shift;
    UPDATE audio_library SET audio_id = audio_id + shift;
    UPDATE playlist_tracks SET audio_id = audio_id + shift;

    CREATE TEMP TABLE audio_map (old_id BIGINT PRIMARY KEY, new_id BIGINT UNIQUE) ON COMMIT DROP;
    FOR rec IN SELECT id FROM audios LOOP
        LOOP
            candidate := 1 + floor(random() * 2147483646)::BIGINT;
            EXIT WHEN NOT EXISTS (SELECT 1 FROM audio_map WHERE new_id = candidate);
        END LOOP;
        INSERT INTO audio_map (old_id, new_id) VALUES (rec.id, candidate);
    END LOOP;

    UPDATE audios t SET id = m.new_id FROM audio_map m WHERE t.id = m.old_id;
    UPDATE audio_library t SET audio_id = m.new_id FROM audio_map m WHERE t.audio_id = m.old_id;
    UPDATE playlist_tracks t SET audio_id = m.new_id FROM audio_map m WHERE t.audio_id = m.old_id;

    UPDATE videos SET id = id + shift;

    CREATE TEMP TABLE video_map (old_id BIGINT PRIMARY KEY, new_id BIGINT UNIQUE) ON COMMIT DROP;
    FOR rec IN SELECT id FROM videos LOOP
        LOOP
            candidate := 1 + floor(random() * 2147483646)::BIGINT;
            EXIT WHEN NOT EXISTS (SELECT 1 FROM video_map WHERE new_id = candidate);
        END LOOP;
        INSERT INTO video_map (old_id, new_id) VALUES (rec.id, candidate);
    END LOOP;

    UPDATE videos t SET id = m.new_id FROM video_map m WHERE t.id = m.old_id;

    FOR rec IN SELECT * FROM saved_fks LOOP
        EXECUTE format('ALTER TABLE %s ADD CONSTRAINT %I %s', rec.tbl, rec.conname, rec.def);
    END LOOP;
END
$remint$;
