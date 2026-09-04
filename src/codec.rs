//! Protobuf HTTP extract/response and domain conversions.

use axum::extract::{FromRequest, Request};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use bytes::Bytes;
use chrono::{DateTime, Utc};
use prost::Message;

use crate::error::AppError;
use crate::modules::about::models::InstanceAbout;
use crate::modules::auth::models::{Credentials, TokenResponse};
use crate::modules::groups::models::Group;
use crate::modules::media::models::{Album, AudioTrack, Photo, Video};
use crate::modules::messenger::models::Message as DomainMessage;
use crate::modules::notifications::models::Notification;
use crate::modules::users::models::{PrivacyLevel, UpdateAccount, User};
use crate::modules::wall::models::WallPost;
use crate::pb;
use crate::security::ChallengeResponse;

pub const PROTOBUF_MIME: &str = "application/x-protobuf";

#[derive(Debug, Clone)]
pub struct Proto<T>(pub T);

impl<S, T> FromRequest<S> for Proto<T>
where
    S: Send + Sync,
    T: Message + Default,
{
    type Rejection = AppError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let content_type = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        if !is_protobuf_mime(content_type) {
            return Err(AppError::UnsupportedMediaType);
        }
        let bytes = Bytes::from_request(req, state)
            .await
            .map_err(|_| AppError::Validation("could not read protobuf body".into()))?;
        let message = T::decode(bytes)
            .map_err(|_| AppError::Validation("request is not a valid protobuf message".into()))?;
        Ok(Self(message))
    }
}

impl<T: Message> IntoResponse for Proto<T> {
    fn into_response(self) -> Response {
        protobuf_response(StatusCode::OK, &self.0)
    }
}

#[must_use]
pub fn is_protobuf_mime(value: &str) -> bool {
    let mime = value
        .split(';')
        .next()
        .unwrap_or(value)
        .trim()
        .to_ascii_lowercase();
    mime == PROTOBUF_MIME || mime == "application/protobuf"
}

pub fn protobuf_response<T: Message>(status: StatusCode, message: &T) -> Response {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(PROTOBUF_MIME),
    );
    (status, headers, message.encode_to_vec()).into_response()
}

pub fn encode<T: Message>(message: &T) -> Vec<u8> {
    message.encode_to_vec()
}

pub fn decode<T: Message + Default>(bytes: &[u8]) -> Result<T, AppError> {
    T::decode(bytes).map_err(|_| AppError::Validation("invalid protobuf".into()))
}

pub fn privacy_from_pb(value: i32) -> PrivacyLevel {
    match pb::PrivacyLevel::try_from(value).unwrap_or(pb::PrivacyLevel::Unspecified) {
        pb::PrivacyLevel::Friends => PrivacyLevel::Friends,
        pb::PrivacyLevel::Nobody => PrivacyLevel::Nobody,
        pb::PrivacyLevel::Everyone | pb::PrivacyLevel::Unspecified => PrivacyLevel::Everyone,
    }
}

pub fn privacy_to_pb(value: PrivacyLevel) -> i32 {
    match value {
        PrivacyLevel::Everyone => pb::PrivacyLevel::Everyone as i32,
        PrivacyLevel::Friends => pb::PrivacyLevel::Friends as i32,
        PrivacyLevel::Nobody => pb::PrivacyLevel::Nobody as i32,
    }
}

fn rfc3339(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

fn rfc3339_opt(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(rfc3339)
}

pub fn user_to_pb(user: &User) -> pb::User {
    pb::User {
        id: user.id,
        first_name: user.first_name.clone(),
        last_name: user.last_name.clone(),
        screen_name: user.screen_name.clone(),
        status: user.status.clone(),
        city: user.city.clone(),
        email: user.email.clone(),
        phone: user.phone.clone(),
        avatar_url: user.avatar_url.clone(),
        verified: user.verified,
        privacy_wall: privacy_to_pb(user.privacy_wall),
        privacy_messages: privacy_to_pb(user.privacy_messages),
        privacy_photos: privacy_to_pb(user.privacy_photos),
        privacy_audio: privacy_to_pb(user.privacy_audio),
        created_at: rfc3339(user.created_at),
    }
}

pub fn users_to_pb(users: Vec<User>) -> pb::UserList {
    pb::UserList {
        users: users.iter().map(user_to_pb).collect(),
    }
}

pub fn wall_post_to_pb(post: &WallPost) -> pb::WallPost {
    pb::WallPost {
        id: post.id,
        target_id: post.target_id,
        author_id: post.author_id,
        author: Some(user_to_pb(&post.author)),
        target: Some(user_to_pb(&post.target)),
        content: post.content.clone(),
        permalink: post.permalink.clone(),
        created_at: rfc3339(post.created_at),
    }
}

pub fn wall_posts_to_pb(posts: Vec<WallPost>) -> pb::WallPostList {
    pb::WallPostList {
        posts: posts.iter().map(wall_post_to_pb).collect(),
    }
}

pub fn message_to_pb(message: &DomainMessage) -> pb::Message {
    pb::Message {
        id: message.id,
        peer_id: message.peer_id,
        author_id: message.author_id,
        text: message.text.clone(),
        created_at: rfc3339(message.created_at),
    }
}

pub fn messages_to_pb(messages: Vec<DomainMessage>) -> pb::MessageList {
    pb::MessageList {
        messages: messages.iter().map(message_to_pb).collect(),
    }
}

pub fn audio_to_pb(track: &AudioTrack) -> pb::AudioTrack {
    pb::AudioTrack {
        id: track.id,
        media_id: track.media_id,
        artist: track.artist.clone(),
        title: track.title.clone(),
        duration_ms: track.duration_ms,
        owner_user_id: track.owner_user_id,
        src: track.src.clone(),
    }
}

pub fn audio_list_to_pb(tracks: Vec<AudioTrack>) -> pb::AudioList {
    pb::AudioList {
        tracks: tracks.iter().map(audio_to_pb).collect(),
    }
}

pub fn photo_to_pb(photo: &Photo) -> pb::Photo {
    pb::Photo {
        id: photo.id,
        album_id: photo.album_id,
        owner_user_id: photo.owner_user_id,
        mime: photo.mime.clone(),
        size_bytes: photo.size_bytes,
        width: photo.width,
        height: photo.height,
        original_filename: photo.original_filename.clone(),
        url: photo.url.clone(),
    }
}

pub fn album_to_pb(album: &Album) -> pb::Album {
    pb::Album {
        id: album.id,
        title: album.title.clone(),
        description: album.description.clone(),
        owner_user_id: album.owner_user_id,
        created_at: rfc3339(album.created_at),
        photo_count: album.photo_count,
        cover_url: album.cover_url.clone(),
        photos: album.photos.iter().map(photo_to_pb).collect(),
    }
}

pub fn albums_to_pb(albums: Vec<Album>) -> pb::AlbumList {
    pb::AlbumList {
        albums: albums.iter().map(album_to_pb).collect(),
    }
}

pub fn video_to_pb(video: &Video) -> pb::Video {
    pb::Video {
        id: video.id,
        media_id: video.media_id,
        title: video.title.clone(),
        description: video.description.clone(),
        status: video.status.clone(),
        owner_user_id: video.owner_user_id,
        src: video.src.clone(),
    }
}

pub fn videos_to_pb(videos: Vec<Video>) -> pb::VideoList {
    pb::VideoList {
        videos: videos.iter().map(video_to_pb).collect(),
    }
}

pub fn group_to_pb(group: &Group) -> pb::Group {
    pb::Group {
        id: group.id,
        slug: group.slug.clone(),
        name: group.name.clone(),
        about: group.about.clone(),
        kind: group.kind.clone(),
        owner_id: group.owner_id,
        created_at: rfc3339(group.created_at),
    }
}

pub fn groups_to_pb(groups: Vec<Group>) -> pb::GroupList {
    pb::GroupList {
        groups: groups.iter().map(group_to_pb).collect(),
    }
}

pub fn instance_about_to_pb(about: &InstanceAbout) -> pb::InstanceAbout {
    pb::InstanceAbout {
        users: about.users,
        online_users: about.online_users,
        active_users: about.active_users,
        groups: about.groups,
        wall_posts: about.wall_posts,
        popular_groups: about
            .popular_groups
            .iter()
            .map(|group| pb::PopularGroup {
                id: group.id,
                name: group.name.clone(),
                members: group.members,
            })
            .collect(),
    }
}

pub fn notification_to_pb(item: &Notification) -> pb::Notification {
    pb::Notification {
        id: item.id,
        kind: item.kind.clone(),
        actor_id: item.actor_id,
        entity_type: item.entity_type.clone(),
        entity_id: item.entity_id,
        payload_json: item.payload.to_string(),
        read_at: rfc3339_opt(item.read_at),
        created_at: rfc3339(item.created_at),
    }
}

pub fn notifications_to_pb(items: Vec<Notification>) -> pb::NotificationList {
    pb::NotificationList {
        notifications: items.iter().map(notification_to_pb).collect(),
    }
}

pub fn challenge_to_pb(issued: &ChallengeResponse) -> pb::Challenge {
    pb::Challenge {
        csrf_token: issued.csrf_token.clone(),
        challenge_id: issued.challenge_id.clone(),
        nonce: issued.nonce.clone(),
        expires_in: issued.expires_in,
        alg: issued.alg.to_owned(),
        public_key: issued.public_key.clone(),
    }
}

pub fn token_to_pb(token: &TokenResponse) -> pb::Token {
    pb::Token {
        token: token.token.clone(),
        token_type: token.token_type.to_owned(),
        user_id: token.user_id,
    }
}

pub fn credentials_from_pb(body: pb::AuthRequest) -> Credentials {
    Credentials {
        login: body.login,
        challenge_id: body.challenge_id,
        password_sealed: body.password_sealed,
        turnstile_token: body.turnstile_token,
    }
}

pub fn update_account_from_pb(body: pb::UpdateAccount) -> UpdateAccount {
    UpdateAccount {
        first_name: body.first_name,
        last_name: body.last_name,
        email: body.email,
        phone: body.phone,
        city: body.city,
        privacy_wall: privacy_from_pb(body.privacy_wall),
        privacy_messages: privacy_from_pb(body.privacy_messages),
    }
}

#[cfg(test)]
mod tests {
    use super::{is_protobuf_mime, privacy_from_pb, privacy_to_pb};
    use crate::modules::users::PrivacyLevel;
    use crate::pb;

    #[test]
    fn accepts_protobuf_content_types() {
        assert!(is_protobuf_mime("application/x-protobuf"));
        assert!(is_protobuf_mime("application/protobuf; charset=utf-8"));
        assert!(!is_protobuf_mime("application/json"));
    }

    #[test]
    fn privacy_roundtrips() {
        for level in [
            PrivacyLevel::Everyone,
            PrivacyLevel::Friends,
            PrivacyLevel::Nobody,
        ] {
            assert_eq!(privacy_from_pb(privacy_to_pb(level)), level);
        }
        assert_eq!(
            privacy_from_pb(pb::PrivacyLevel::Unspecified as i32),
            PrivacyLevel::Everyone
        );
    }
}
