-- Media is read from the bucket host (nginx or CDN), so the database keeps the
-- storage key and the API builds the URL from MEDIA_PUBLIC_BASE_URL. Absolute
-- URLs in rows would freeze the old host into every avatar.
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'users' AND column_name = 'avatar_url'
    ) THEN
        ALTER TABLE users RENAME COLUMN avatar_url TO avatar_key;
    END IF;
END $$;

UPDATE users
SET avatar_key = regexp_replace(avatar_key, '^https?://[^/]+/', '')
WHERE avatar_key LIKE 'http://%' OR avatar_key LIKE 'https://%';

UPDATE users
SET avatar_key = ltrim(avatar_key, '/')
WHERE avatar_key LIKE '/%';

UPDATE users
SET avatar_key = substring(avatar_key FROM length('media/') + 1)
WHERE avatar_key LIKE 'media/%';
