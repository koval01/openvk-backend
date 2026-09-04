//! Signs real requests against a real S3 endpoint: Silo, `MinIO`, or R2.
//! Skipped unless `S3_ENDPOINT` and credentials are in the environment
//! (`docker compose --profile dev up -d silo` provides them locally).

use bytes::Bytes;
use openvk_backend::{Config, Storage, StorageBackend};

#[tokio::test]
async fn s3_put_get_delete_roundtrip() {
    dotenvy::dotenv().ok();
    let mut config = Config::from_env().expect("config");
    if config.s3.is_none() {
        eprintln!("skip s3_put_get_delete_roundtrip: S3 credentials are not configured");
        return;
    }
    config.storage_backend = StorageBackend::S3;
    let storage = Storage::from_config(&config).expect("storage");
    assert_eq!(storage.backend_name(), "s3");

    let key = format!(
        "probe/{}/ping.txt",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );

    assert!(!storage.exists(&key).await.expect("exists before put"));
    storage
        .put(&key, Bytes::from_static(b"openvk-s3"), "text/plain")
        .await
        .expect("put");

    let got = storage.get(&key).await.expect("get");
    assert_eq!(got.bytes.as_ref(), b"openvk-s3");
    assert!(got.content_type.starts_with("text/plain"));
    assert!(storage.exists(&key).await.expect("exists after put"));

    storage.delete(&key).await.expect("delete");
    assert!(!storage.exists(&key).await.expect("exists after delete"));
    // Deleting an object that is already gone is not an error.
    storage.delete(&key).await.expect("idempotent delete");
    assert!(matches!(
        storage.get(&key).await,
        Err(openvk_backend::AppError::NotFound)
    ));
}

#[tokio::test]
async fn s3_deletes_a_whole_album_worth_of_keys() {
    dotenvy::dotenv().ok();
    let mut config = Config::from_env().expect("config");
    if config.s3.is_none() {
        eprintln!("skip s3_deletes_a_whole_album_worth_of_keys: S3 is not configured");
        return;
    }
    config.storage_backend = StorageBackend::S3;
    let storage = Storage::from_config(&config).expect("storage");

    let run = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let keys: Vec<String> = (0..12)
        .map(|index| format!("probe/{run}/album/{index}.txt"))
        .collect();
    for key in &keys {
        storage
            .put(key, Bytes::from_static(b"x"), "text/plain")
            .await
            .expect("put");
    }

    storage.delete_keys(&keys).await.expect("delete keys");
    for key in &keys {
        assert!(
            !storage.exists(key).await.expect("exists"),
            "{key} survived"
        );
    }
}
