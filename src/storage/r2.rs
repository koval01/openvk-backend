use bytes::Bytes;
use reqwest::StatusCode;

use crate::config::Config;
use crate::error::AppError;
use crate::storage::ObjectBody;

#[derive(Clone)]
pub struct R2Store {
    client: reqwest::Client,
    account_id: String,
    bucket: String,
    token: String,
}

impl R2Store {
    pub fn from_config(config: &Config) -> Result<Self, AppError> {
        let account_id = config
            .cloudflare_account_id
            .clone()
            .ok_or_else(|| AppError::Config("CLOUDFLARE_ACCOUNT_ID is required".into()))?;
        let bucket = config
            .r2_bucket
            .clone()
            .ok_or_else(|| AppError::Config("R2_BUCKET is required".into()))?;
        let token = config
            .cloudflare_api_token
            .clone()
            .ok_or_else(|| AppError::Config("CLOUDFLARE_API_TOKEN is required".into()))?;
        Ok(Self {
            client: reqwest::Client::new(),
            account_id,
            bucket,
            token,
        })
    }

    fn object_url(&self, key: &str) -> Result<String, AppError> {
        crate::storage::validate_storage_key(key)?;
        Ok(format!(
            "https://api.cloudflare.com/client/v4/accounts/{}/r2/buckets/{}/objects/{key}",
            self.account_id, self.bucket
        ))
    }

    pub async fn put(&self, key: &str, bytes: Bytes, content_type: &str) -> Result<(), AppError> {
        let url = self.object_url(key)?;
        let response = self
            .client
            .put(url)
            .bearer_auth(&self.token)
            .header("Content-Type", content_type)
            .body(bytes)
            .send()
            .await
            .map_err(|err| AppError::internal(format!("r2 put failed: {err}")))?;
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::internal(format!(
                "r2 put rejected ({status}): {body}"
            )));
        }
        Ok(())
    }

    pub async fn get(&self, key: &str) -> Result<ObjectBody, AppError> {
        let url = self.object_url(key)?;
        let response = self
            .client
            .get(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|err| AppError::internal(format!("r2 get failed: {err}")))?;
        if response.status() == StatusCode::NOT_FOUND {
            return Err(AppError::NotFound);
        }
        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::internal(format!(
                "r2 get rejected ({status}): {body}"
            )));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("application/octet-stream")
            .to_owned();
        let bytes = response
            .bytes()
            .await
            .map_err(|err| AppError::internal(format!("r2 get body failed: {err}")))?;
        Ok(ObjectBody {
            bytes,
            content_type,
        })
    }

    pub async fn delete(&self, key: &str) -> Result<(), AppError> {
        let url = self.object_url(key)?;
        let response = self
            .client
            .delete(url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|err| AppError::internal(format!("r2 delete failed: {err}")))?;
        if response.status() == StatusCode::NOT_FOUND || response.status().is_success() {
            return Ok(());
        }
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(AppError::internal(format!(
            "r2 delete rejected ({status}): {body}"
        )))
    }

    pub async fn exists(&self, key: &str) -> Result<bool, AppError> {
        match self.get(key).await {
            Ok(_) => Ok(true),
            Err(AppError::NotFound) => Ok(false),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::R2Store;
    use crate::error::AppError;

    #[test]
    fn rejects_unsafe_keys() {
        let store = R2Store {
            client: reqwest::Client::new(),
            account_id: "acct".into(),
            bucket: "bucket".into(),
            token: "token".into(),
        };
        assert!(matches!(
            store.object_url("../etc/passwd"),
            Err(AppError::Validation(_))
        ));
        assert!(store.object_url("ok/photo/a.png").is_ok());
    }
}
