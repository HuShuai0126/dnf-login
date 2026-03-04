use anyhow::Result;
use dnf_shared::{crypto::AesGcmCipher, protocol::Request};
use reqwest::Client;

#[derive(Clone)]
pub struct DnfClient {
    client: Client,
    server_url: String,
    cipher: AesGcmCipher,
}

impl DnfClient {
    pub fn new(server_url: String, aes_key: &[u8; 32]) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()?,
            server_url,
            cipher: AesGcmCipher::new(aes_key),
        })
    }

    async fn post_encrypted(&self, pipe_request: &str) -> Result<String> {
        let encrypted = self.cipher.encrypt_string(pipe_request)?;

        let response = self
            .client
            .post(format!("{}/api/v1/auth", self.server_url))
            .header("Content-Type", "application/octet-stream")
            .body(encrypted)
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("HTTP error: {}", response.status());
        }

        let encrypted_response = response.text().await?;
        let plaintext = self.cipher.decrypt_string(&encrypted_response)?;
        Ok(plaintext)
    }

    pub async fn login(
        &self,
        username: &str,
        password_md5: &str,
        mac_address: &str,
    ) -> Result<LoginResponse> {
        let request = Request::Login {
            username: username.to_string(),
            password_md5: password_md5.to_string(),
            mac_address: mac_address.to_string(),
        };

        let plaintext = self.post_encrypted(&request.encode()).await?;

        if let Some(token) = plaintext.strip_prefix("0|") {
            Ok(LoginResponse {
                success: true,
                token: Some(token.to_string()),
                error: None,
            })
        } else {
            Ok(LoginResponse {
                success: false,
                token: None,
                error: Some(plaintext),
            })
        }
    }

    pub async fn register(
        &self,
        username: &str,
        password_md5: &str,
        qq_number: Option<String>,
    ) -> Result<RegisterResponse> {
        let request = Request::Register {
            username: username.to_string(),
            password_md5: password_md5.to_string(),
            qq_number,
        };

        let plaintext = self.post_encrypted(&request.encode()).await?;

        if plaintext == "success" {
            Ok(RegisterResponse {
                success: true,
                error: None,
            })
        } else {
            Ok(RegisterResponse {
                success: false,
                error: Some(plaintext),
            })
        }
    }

    pub async fn change_password(
        &self,
        username: &str,
        old_password_md5: &str,
        new_password_md5: &str,
    ) -> Result<SimpleResponse> {
        let request = Request::ChangePassword {
            username: username.to_string(),
            old_password_md5: old_password_md5.to_string(),
            new_password_md5: new_password_md5.to_string(),
        };

        let plaintext = self.post_encrypted(&request.encode()).await?;

        if plaintext == "success" {
            Ok(SimpleResponse {
                success: true,
                error: None,
            })
        } else {
            Ok(SimpleResponse {
                success: false,
                error: Some(plaintext),
            })
        }
    }
}

#[derive(Debug, Clone)]
pub struct LoginResponse {
    pub success: bool,
    pub token: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RegisterResponse {
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SimpleResponse {
    pub success: bool,
    pub error: Option<String>,
}

pub fn md5_hash(input: &str) -> String {
    format!("{:x}", md5::compute(input))
}
