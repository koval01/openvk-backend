-- Avatars used to point at the API byte-stream route. Files are public under /media/{storage_key}.
UPDATE users AS u
SET avatar_url = '/media/' || m.storage_key
FROM media_objects AS m
WHERE u.avatar_url LIKE '%/api/v1/media/' || m.id || '/content%';
