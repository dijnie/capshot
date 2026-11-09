//! Google Drive uploads with the user's own OAuth client (winshot's model):
//! browser sign-in with PKCE on a loopback address, refresh tokens in the
//! keychain, multipart upload, then an "anyone with the link" reader
//! permission.

use crate::credentials::{self, SecretStore};
use crate::{check_size, content_type, http_client, http_err, CloudError, Result};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const SCOPE: &str = "https://www.googleapis.com/auth/drive.file";
pub const AUTH_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone)]
pub struct Endpoints {
    pub auth: String,
    pub token: String,
    pub revoke: String,
    pub api: String,
    pub upload: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            auth: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token: "https://oauth2.googleapis.com/token".into(),
            revoke: "https://oauth2.googleapis.com/revoke".into(),
            api: "https://www.googleapis.com/drive/v3".into(),
            upload: "https://www.googleapis.com/upload/drive/v3".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Token {
    pub access_token: String,
    pub refresh_token: Option<String>,
    /// Unix seconds.
    pub expires_at: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn random_string(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    getrandom::fill(&mut buf).expect("OS random source");
    URL_SAFE_NO_PAD.encode(buf)
}

fn encode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

pub struct GDrive<'a> {
    store: &'a dyn SecretStore,
    folder_id: String,
    endpoints: Endpoints,
}

/// A started sign-in: open `url` in the browser, then call [`AuthSession::wait`].
pub struct AuthSession {
    pub url: String,
    listener: TcpListener,
    state: String,
    verifier: String,
    redirect: String,
}

impl<'a> GDrive<'a> {
    pub fn new(store: &'a dyn SecretStore, folder_id: &str) -> Self {
        Self { store, folder_id: folder_id.to_string(), endpoints: Endpoints::default() }
    }

    pub fn with_endpoints(mut self, endpoints: Endpoints) -> Self {
        self.endpoints = endpoints;
        self
    }

    pub fn has_client(&self) -> bool {
        matches!(self.store.get(credentials::GDRIVE_CLIENT_ID), Ok(Some(v)) if !v.is_empty())
    }

    fn client(&self) -> Result<(String, String)> {
        Ok((
            credentials::require(self.store, credentials::GDRIVE_CLIENT_ID, "Google Drive client ID")?,
            credentials::require(self.store, credentials::GDRIVE_CLIENT_SECRET, "Google Drive client secret")?,
        ))
    }

    /// Prepare a browser sign-in on a random loopback port.
    pub fn start_auth(&self) -> Result<AuthSession> {
        let (client_id, _) = self.client()?;
        let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| CloudError::Other(e.to_string()))?;
        let port = listener.local_addr().map_err(|e| CloudError::Other(e.to_string()))?.port();
        let redirect = format!("http://127.0.0.1:{port}/callback");
        let verifier = random_string(48);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let state = random_string(24);
        let url = format!(
            "{}?client_id={}&redirect_uri={}&response_type=code&scope={}&access_type=offline&prompt=consent&code_challenge={}&code_challenge_method=S256&state={}",
            self.endpoints.auth,
            encode(&client_id),
            encode(&redirect),
            encode(SCOPE),
            challenge,
            state
        );
        Ok(AuthSession { url, listener, state, verifier, redirect })
    }

    /// Wait for the browser to return, then exchange the code and store tokens.
    pub fn finish_auth(&self, session: AuthSession, timeout: Duration) -> Result<()> {
        let code = session.wait_for_code(timeout)?;
        let (client_id, client_secret) = self.client()?;
        let form = [
            ("code", code.as_str()),
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("redirect_uri", session.redirect.as_str()),
            ("grant_type", "authorization_code"),
            ("code_verifier", session.verifier.as_str()),
        ];
        let token = self.token_request(&form, None)?;
        self.save_token(&token)
    }

    fn token_request(&self, form: &[(&str, &str)], keep_refresh: Option<String>) -> Result<Token> {
        let r = http_client()?.post(&self.endpoints.token).form(form).send().map_err(http_err)?;
        let status = r.status().as_u16();
        let body = r.text().unwrap_or_default();
        if !(200..300).contains(&status) {
            return Err(CloudError::Status { service: "Google sign-in", status, body });
        }
        let t: TokenResponse = serde_json::from_str(&body).map_err(|e| CloudError::Other(e.to_string()))?;
        Ok(Token {
            access_token: t.access_token,
            refresh_token: t.refresh_token.or(keep_refresh),
            expires_at: now() + t.expires_in.unwrap_or(3600).saturating_sub(60),
        })
    }

    fn save_token(&self, t: &Token) -> Result<()> {
        self.store
            .set(credentials::GDRIVE_TOKEN, &serde_json::to_string(t).map_err(|e| CloudError::Other(e.to_string()))?)
    }

    fn load_token(&self) -> Result<Option<Token>> {
        Ok(self.store.get(credentials::GDRIVE_TOKEN)?.and_then(|s| serde_json::from_str(&s).ok()))
    }

    pub fn is_connected(&self) -> bool {
        matches!(self.load_token(), Ok(Some(_)))
    }

    /// A valid access token, refreshing it when it has expired.
    fn access_token(&self) -> Result<String> {
        let t = self.load_token()?.ok_or(CloudError::NotConfigured("Google Drive sign-in"))?;
        if t.expires_at > now() {
            return Ok(t.access_token);
        }
        let refresh = t.refresh_token.clone().ok_or(CloudError::NotConfigured("Google Drive sign-in"))?;
        let (client_id, client_secret) = self.client()?;
        let form = [
            ("client_id", client_id.as_str()),
            ("client_secret", client_secret.as_str()),
            ("refresh_token", refresh.as_str()),
            ("grant_type", "refresh_token"),
        ];
        let fresh = self.token_request(&form, Some(refresh.clone()))?;
        self.save_token(&fresh)?;
        Ok(fresh.access_token)
    }

    /// Signed-in account email.
    pub fn account_email(&self) -> Result<String> {
        let token = self.access_token()?;
        let r = http_client()?
            .get(format!("{}/about?fields=user(emailAddress)", self.endpoints.api))
            .bearer_auth(token)
            .send()
            .map_err(http_err)?;
        let status = r.status().as_u16();
        let body = r.text().unwrap_or_default();
        if !(200..300).contains(&status) {
            return Err(CloudError::Status { service: "Google Drive", status, body });
        }
        let v: serde_json::Value = serde_json::from_str(&body).map_err(|e| CloudError::Other(e.to_string()))?;
        Ok(v["user"]["emailAddress"].as_str().unwrap_or_default().to_string())
    }

    /// Upload, share with "anyone with the link", and return the view URL.
    pub fn upload(&self, data: &[u8], filename: &str) -> Result<String> {
        check_size(data)?;
        let token = self.access_token()?;
        let mime = content_type(filename);
        let mut meta = serde_json::json!({ "name": filename, "mimeType": mime });
        if !self.folder_id.is_empty() {
            meta["parents"] = serde_json::json!([self.folder_id]);
        }
        let boundary = format!("capshot-{}", random_string(12));
        let mut body = Vec::with_capacity(data.len() + 512);
        write!(body, "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: {mime}\r\n\r\n").unwrap();
        body.extend_from_slice(data);
        write!(body, "\r\n--{boundary}--\r\n").unwrap();

        let client = http_client()?;
        let r = client
            .post(format!("{}/files?uploadType=multipart&fields=id", self.endpoints.upload))
            .bearer_auth(&token)
            .header("Content-Type", format!("multipart/related; boundary={boundary}"))
            .body(body)
            .send()
            .map_err(http_err)?;
        let status = r.status().as_u16();
        let text = r.text().unwrap_or_default();
        if !(200..300).contains(&status) {
            return Err(CloudError::Status { service: "Google Drive", status, body: text });
        }
        let id = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|v| v["id"].as_str().map(String::from))
            .ok_or_else(|| CloudError::Other("Drive did not return a file id".into()))?;

        let r = client
            .post(format!("{}/files/{id}/permissions", self.endpoints.api))
            .bearer_auth(&token)
            .json(&serde_json::json!({ "type": "anyone", "role": "reader" }))
            .send()
            .map_err(http_err)?;
        let link = format!("https://drive.google.com/file/d/{id}/view");
        if !r.status().is_success() {
            // The file exists but others cannot open the link; say so instead of
            // handing out a link that only works for the owner.
            return Err(CloudError::Other(format!(
                "uploaded, but sharing failed ({}); the link only works for you: {link}",
                r.status()
            )));
        }
        Ok(link)
    }

    /// Revoke the token (best effort) and forget it.
    pub fn disconnect(&self) -> Result<()> {
        if let Ok(Some(t)) = self.load_token() {
            let tok = t.refresh_token.unwrap_or(t.access_token);
            if let Ok(c) = http_client() {
                let _ = c.post(&self.endpoints.revoke).form(&[("token", tok.as_str())]).send();
            }
        }
        self.store.delete(credentials::GDRIVE_TOKEN)
    }
}

const DONE_PAGE: &str = "<html><body style=\"font-family:sans-serif;padding:40px\"><h2>capshot is connected to Google Drive</h2><p>You can close this tab.</p></body></html>";
const FAIL_PAGE: &str = "<html><body style=\"font-family:sans-serif;padding:40px\"><h2>Sign-in failed</h2><p>Return to capshot and try again.</p></body></html>";

impl AuthSession {
    /// Accept loopback requests until the callback arrives or `timeout` passes.
    /// Requests with a wrong `state` are rejected.
    fn wait_for_code(&self, timeout: Duration) -> Result<String> {
        self.listener.set_nonblocking(true).map_err(|e| CloudError::Other(e.to_string()))?;
        let deadline = Instant::now() + timeout;
        while Instant::now() < deadline {
            let (mut stream, _) = match self.listener.accept() {
                Ok(s) => s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(100));
                    continue;
                }
                Err(e) => return Err(CloudError::Other(e.to_string())),
            };
            let _ = stream.set_nonblocking(false);
            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
            let mut buf = [0u8; 8192];
            let n = stream.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let target = req.split_whitespace().nth(1).unwrap_or("");
            let Some(query) = target.strip_prefix("/callback?") else {
                let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
                continue;
            };
            let params: std::collections::HashMap<String, String> =
                url::form_urlencoded::parse(query.as_bytes()).into_owned().collect();
            let state_ok = params.get("state") == Some(&self.state);
            let ok = state_ok && params.contains_key("code");
            let page = if ok { DONE_PAGE } else { FAIL_PAGE };
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}", page.len());
            if ok {
                return Ok(params["code"].clone());
            }
            // Only a callback carrying our state may cancel the sign-in.
            if let (true, Some(err)) = (state_ok, params.get("error")) {
                return Err(CloudError::Other(format!("Google sign-in: {err}")));
            }
        }
        Err(CloudError::Other("Google sign-in timed out".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::MemoryStore;
    use crate::mock;

    fn store_with_client() -> MemoryStore {
        let s = MemoryStore::default();
        s.set(credentials::GDRIVE_CLIENT_ID, "cid.apps.googleusercontent.com").unwrap();
        s.set(credentials::GDRIVE_CLIENT_SECRET, "csecret").unwrap();
        s
    }

    fn endpoints(base: &str) -> Endpoints {
        Endpoints {
            auth: format!("{base}/auth"),
            token: format!("{base}/token"),
            revoke: format!("{base}/revoke"),
            api: base.to_string(),
            upload: format!("{base}/upload"),
        }
    }

    #[test]
    fn auth_url_has_pkce_and_loopback_redirect() {
        let store = store_with_client();
        let g = GDrive::new(&store, "");
        let s = g.start_auth().unwrap();
        assert!(s.url.contains("code_challenge_method=S256"));
        assert!(s.url.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A"));
        assert!(s.url.contains("access_type=offline"));
        assert!(s.url.contains(&format!("state={}", s.state)));
    }

    #[test]
    fn sign_in_round_trip_stores_tokens() {
        let store = store_with_client();
        let m = mock::serve(vec![(200, r#"{"access_token":"AT","refresh_token":"RT","expires_in":3600}"#)]);
        let g = GDrive::new(&store, "").with_endpoints(endpoints(&m.url));
        let session = g.start_auth().unwrap();
        let callback = format!("{}?code=THECODE&state={}", session.redirect, session.state);
        // Play the browser: one request with a wrong state, then the real one.
        let bad = format!("{}?code=X&state=wrong", session.redirect);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            let _ = reqwest::blocking::get(bad);
            let _ = reqwest::blocking::get(callback);
        });
        g.finish_auth(session, Duration::from_secs(10)).unwrap();
        let req = &m.requests.lock().unwrap()[0];
        let body = String::from_utf8_lossy(&req.body);
        assert!(body.contains("code=THECODE") && body.contains("code_verifier="), "{body}");
        let t = g.load_token().unwrap().unwrap();
        assert_eq!((t.access_token.as_str(), t.refresh_token.as_deref()), ("AT", Some("RT")));
    }

    #[test]
    fn expired_token_is_refreshed_before_upload() {
        let store = store_with_client();
        let expired = Token { access_token: "OLD".into(), refresh_token: Some("RT".into()), expires_at: 1 };
        store.set(credentials::GDRIVE_TOKEN, &serde_json::to_string(&expired).unwrap()).unwrap();
        let m = mock::serve(vec![
            (200, r#"{"access_token":"NEW","expires_in":3600}"#),
            (200, r#"{"id":"FILE123"}"#),
            (200, r#"{"id":"perm"}"#),
        ]);
        let g = GDrive::new(&store, "FOLDER").with_endpoints(endpoints(&m.url));
        let url = g.upload(b"IMG", "a.png").unwrap();
        assert_eq!(url, "https://drive.google.com/file/d/FILE123/view");
        let reqs = m.requests.lock().unwrap();
        assert!(String::from_utf8_lossy(&reqs[0].body).contains("grant_type=refresh_token"));
        assert_eq!(reqs[1].header("authorization"), Some("Bearer NEW"));
        assert!(reqs[1].path.starts_with("/upload/files?uploadType=multipart"));
        let up = String::from_utf8_lossy(&reqs[1].body);
        assert!(up.contains("\"parents\":[\"FOLDER\"]") && up.contains("IMG"));
        assert_eq!(reqs[2].path, "/files/FILE123/permissions");
        assert!(String::from_utf8_lossy(&reqs[2].body).contains("\"anyone\""));
        // The refresh token survives a refresh response that omits it.
        assert_eq!(g.load_token().unwrap().unwrap().refresh_token.as_deref(), Some("RT"));
    }

    #[test]
    fn failed_sharing_is_an_error() {
        let store = store_with_client();
        let fresh = Token { access_token: "AT".into(), refresh_token: None, expires_at: u64::MAX };
        store.set(credentials::GDRIVE_TOKEN, &serde_json::to_string(&fresh).unwrap()).unwrap();
        let m = mock::serve(vec![(200, r#"{"id":"F"}"#), (403, r#"{"error":"forbidden"}"#)]);
        let g = GDrive::new(&store, "").with_endpoints(endpoints(&m.url));
        let err = g.upload(b"IMG", "a.png").unwrap_err().to_string();
        assert!(err.contains("sharing failed") && err.contains("/file/d/F/view"), "{err}");
    }

    #[test]
    fn upload_without_sign_in_is_reported() {
        let store = store_with_client();
        let g = GDrive::new(&store, "");
        assert!(matches!(g.upload(b"x", "a.png"), Err(CloudError::NotConfigured(_))));
    }
}
