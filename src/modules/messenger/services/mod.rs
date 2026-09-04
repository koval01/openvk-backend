use crate::error::AppError;
use crate::modules::friends::repository::FriendshipRepository;
use crate::modules::messenger::models::Message;
use crate::modules::messenger::repository::MessageRepository;
use crate::modules::users::PrivacyLevel;
use crate::state::AppState;

pub async fn list_inbox(state: &AppState, user_id: i64) -> Result<Vec<Message>, AppError> {
    MessageRepository::new(&state.db, &state.vault)
        .list_inbox(user_id)
        .await
}

pub async fn list_thread(
    state: &AppState,
    user_id: i64,
    peer_id: i64,
) -> Result<Vec<Message>, AppError> {
    MessageRepository::new(&state.db, &state.vault)
        .list_thread(user_id, peer_id)
        .await
}

pub async fn send(
    state: &AppState,
    author_id: i64,
    peer_id: i64,
    text: String,
) -> Result<Message, AppError> {
    let text = text.trim().to_owned();
    if text.is_empty() {
        return Err(AppError::Validation("message cannot be empty".into()));
    }
    if text.chars().count() > 4096 {
        return Err(AppError::Validation(
            "message is longer than 4096 characters".into(),
        ));
    }

    let author = state
        .users()
        .find_by_id(author_id)
        .await?
        .ok_or(AppError::Unauthorized)?;
    if !author.messaging_allowed && !author.is_agent() {
        return Err(AppError::Forbidden);
    }

    let peer = state
        .users()
        .find_by_id(peer_id)
        .await?
        .ok_or(AppError::NotFound)?;
    let is_friend = FriendshipRepository::new(&state.db)
        .is_friend(author_id, peer_id)
        .await?;
    if !can_write_messages(author_id, peer.id, peer.privacy_messages, is_friend) {
        return Err(AppError::Forbidden);
    }

    MessageRepository::new(&state.db, &state.vault)
        .send(author_id, peer_id, &text)
        .await
}

fn can_write_messages(
    author_id: i64,
    peer_id: i64,
    privacy: PrivacyLevel,
    is_friend: bool,
) -> bool {
    if author_id == peer_id {
        return true;
    }
    match privacy {
        PrivacyLevel::Everyone => true,
        PrivacyLevel::Friends => is_friend,
        PrivacyLevel::Nobody => false,
    }
}
