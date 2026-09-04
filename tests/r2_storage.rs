use bytes::Bytes;
use openvk_backend::{Config, Storage, StorageBackend};

#[tokio::test]
async fn r2_put_get_delete_roundtrip() {
    dotenvy::dotenv().ok();
    let mut config = Config::from_env().expect("config");
    if config.cloudflare_account_id.is_none()
        || config.cloudflare_api_token.is_none()
        || config.r2_bucket.is_none()
    {
        eprintln!("skip r2_put_get_delete_roundtrip: R2 credentials are not configured");
        return;
    }
    config.storage_backend = StorageBackend::R2;
    let storage = Storage::from_config(&config).expect("storage");
    assert_eq!(storage.backend_name(), "r2");

    let key = format!(
        "probe/{}/ping.txt",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    storage
        .put(&key, Bytes::from_static(b"openvk-r2"), "text/plain")
        .await
        .expect("put");
    let got = storage.get(&key).await.expect("get");
    assert_eq!(got.bytes.as_ref(), b"openvk-r2");
    assert!(storage.exists(&key).await.unwrap());
    storage.delete(&key).await.expect("delete");
    assert!(!storage.exists(&key).await.unwrap());
}
