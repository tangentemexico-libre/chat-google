use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use tiny_http::{Response, Server};
use url::Url;

const SCOPES: &[&str] = &[
    "openid",
    "https://www.googleapis.com/auth/userinfo.email",
    "https://www.googleapis.com/auth/userinfo.profile",
    "https://www.googleapis.com/auth/chat.spaces.readonly",
    "https://www.googleapis.com/auth/chat.spaces.create",
    "https://www.googleapis.com/auth/chat.messages",
    "https://www.googleapis.com/auth/chat.memberships.readonly",
    "https://www.googleapis.com/auth/chat.users.availability.readonly",
    "https://www.googleapis.com/auth/contacts.readonly",
    "https://www.googleapis.com/auth/contacts.other.readonly",
    "https://www.googleapis.com/auth/directory.readonly",
];

/// Loopback IPv4 fijo (evita que "localhost" escuche solo en ::1 y el cancel no llegue).
const OAUTH_PORT: u16 = 42813;
const OAUTH_HOST: &str = "127.0.0.1";

static OAUTH_CANCEL: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Deserialize)]
struct Installed {
    client_id: String,
    client_secret: String,
    auth_uri: String,
    token_uri: String,
}

#[derive(Debug, Deserialize)]
struct OAuthFile {
    installed: Option<Installed>,
    web: Option<Installed>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenInfo {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: u64,
    pub email: Option<String>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSummary {
    pub email: String,
    pub name: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct AccountsStore {
    active_email: Option<String>,
    /// Clave = email en minúsculas
    accounts: BTreeMap<String, TokenInfo>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct UserInfo {
    email: Option<String>,
    name: Option<String>,
}

fn app_data_dir(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map_err(|e| format!("No se pudo resolver app data: {e}"))
}

fn credentials_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("google-oauth.client.json"))
}

fn tokens_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("tokens.json"))
}

fn accounts_path(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_dir(app)?.join("accounts.json"))
}

fn normalize_email(email: &str) -> String {
    email.trim().to_lowercase()
}

fn load_oauth_client(app: &AppHandle) -> Result<Installed, String> {
    let path = credentials_path(app)?;
    if !path.exists() {
        return Err(format!(
            "Falta el archivo OAuth Desktop: {}. Descárgalo desde Google Cloud Console (tipo Desktop) y guárdalo ahí.",
            path.display()
        ));
    }
    let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let parsed: OAuthFile = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
    parsed
        .installed
        .or(parsed.web)
        .ok_or_else(|| "El JSON OAuth no tiene bloque installed/web".into())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn pkce_pair() -> (String, String) {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    let verifier = URL_SAFE_NO_PAD.encode(bytes);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

fn save_store(app: &AppHandle, store: &AccountsStore) -> Result<(), String> {
    let dir = app_data_dir(app)?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = accounts_path(app)?;
    let json = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())?;
    Ok(())
}

fn migrate_legacy_tokens(app: &AppHandle, store: &mut AccountsStore) -> Result<(), String> {
    let legacy = tokens_path(app)?;
    if !legacy.exists() {
        return Ok(());
    }
    let raw = fs::read_to_string(&legacy).map_err(|e| e.to_string())?;
    if let Ok(token) = serde_json::from_str::<TokenInfo>(&raw) {
        if let Some(email) = token.email.as_ref().map(|e| normalize_email(e)) {
            if !email.is_empty() && !store.accounts.contains_key(&email) {
                let mut t = token;
                t.email = Some(email.clone());
                store.accounts.insert(email.clone(), t);
                if store.active_email.is_none() {
                    store.active_email = Some(email);
                }
            }
        }
    }
    let _ = fs::remove_file(legacy);
    Ok(())
}

fn load_store(app: &AppHandle) -> Result<AccountsStore, String> {
    let path = accounts_path(app)?;
    let mut store = if path.exists() {
        let raw = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str::<AccountsStore>(&raw).unwrap_or_default()
    } else {
        AccountsStore::default()
    };

    migrate_legacy_tokens(app, &mut store)?;

    // Si hay cuentas pero no activa, elige la primera.
    if store.active_email.is_none() {
        if let Some(email) = store.accounts.keys().next().cloned() {
            store.active_email = Some(email);
        }
    }

    // Si la activa ya no existe, reasigna.
    if let Some(active) = store.active_email.clone() {
        if !store.accounts.contains_key(&active) {
            store.active_email = store.accounts.keys().next().cloned();
        }
    }

    Ok(store)
}

fn upsert_account(store: &mut AccountsStore, tokens: TokenInfo) -> Result<TokenInfo, String> {
    let email = tokens
        .email
        .as_ref()
        .map(|e| normalize_email(e))
        .filter(|e| !e.is_empty())
        .ok_or_else(|| "Google no devolvió el correo de la cuenta.".to_string())?;

    let mut saved = tokens;
    saved.email = Some(email.clone());

    if saved.refresh_token.is_none() {
        if let Some(prev) = store.accounts.get(&email) {
            saved.refresh_token = prev.refresh_token.clone();
        }
    }
    if saved.name.is_none() {
        if let Some(prev) = store.accounts.get(&email) {
            saved.name = prev.name.clone();
        }
    }

    store.accounts.insert(email.clone(), saved.clone());
    store.active_email = Some(email);
    Ok(saved)
}

pub fn save_tokens(app: &AppHandle, tokens: &TokenInfo) -> Result<(), String> {
    let mut store = load_store(app)?;
    let saved = upsert_account(&mut store, tokens.clone())?;
    save_store(app, &store)?;
    let _ = saved;
    Ok(())
}

pub fn load_tokens(app: &AppHandle) -> Result<Option<TokenInfo>, String> {
    let store = load_store(app)?;
    let Some(active) = store.active_email.as_ref() else {
        return Ok(None);
    };
    Ok(store.accounts.get(active).cloned())
}

pub fn list_accounts(app: &AppHandle) -> Result<Vec<AccountSummary>, String> {
    let store = load_store(app)?;
    let active = store.active_email.clone();
    Ok(store
        .accounts
        .iter()
        .map(|(email, token)| AccountSummary {
            email: email.clone(),
            name: token.name.clone(),
            active: active.as_ref() == Some(email),
        })
        .collect())
}

pub fn switch_account(app: &AppHandle, email: String) -> Result<TokenInfo, String> {
    let key = normalize_email(&email);
    let mut store = load_store(app)?;
    let token = store
        .accounts
        .get(&key)
        .cloned()
        .ok_or_else(|| format!("No hay sesión guardada para {key}"))?;
    store.active_email = Some(key);
    save_store(app, &store)?;
    Ok(token)
}

/// Quita una cuenta. Si era la activa, pasa a otra o deja sin sesión.
pub fn remove_account(app: &AppHandle, email: String) -> Result<Option<TokenInfo>, String> {
    let key = normalize_email(&email);
    let mut store = load_store(app)?;
    if store.accounts.remove(&key).is_none() {
        return Err(format!("No hay sesión guardada para {key}"));
    }
    if store.active_email.as_ref() == Some(&key) {
        store.active_email = store.accounts.keys().next().cloned();
    }
    save_store(app, &store)?;
    Ok(store
        .active_email
        .as_ref()
        .and_then(|e| store.accounts.get(e).cloned()))
}

/// Cierra solo la cuenta activa. Si quedan otras, cambia a la siguiente.
pub fn clear_tokens(app: &AppHandle) -> Result<Option<TokenInfo>, String> {
    let store = load_store(app)?;
    let Some(active) = store.active_email.clone() else {
        return Ok(None);
    };
    remove_account(app, active)
}

pub fn credentials_status(app: &AppHandle) -> Result<serde_json::Value, String> {
    let path = credentials_path(app)?;
    Ok(serde_json::json!({
        "path": path.display().to_string(),
        "exists": path.exists(),
        "redirect_uri": format!("http://{OAUTH_HOST}:{OAUTH_PORT}"),
    }))
}

async fn fetch_userinfo(access_token: &str) -> Result<UserInfo, String> {
    let client = reqwest::Client::new();
    client
        .get("https://www.googleapis.com/oauth2/v3/userinfo")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json::<UserInfo>()
        .await
        .map_err(|e| e.to_string())
}

async fn exchange_code(
    client: &Installed,
    code: &str,
    redirect_uri: &str,
    verifier: &str,
) -> Result<TokenInfo, String> {
    let http = reqwest::Client::new();
    let resp = http
        .post(&client.token_uri)
        .form(&[
            ("code", code),
            ("client_id", client.client_id.as_str()),
            ("client_secret", client.client_secret.as_str()),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
            ("code_verifier", verifier),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Error al canjear código OAuth: {body}"));
    }

    let token: TokenResponse = resp.json().await.map_err(|e| e.to_string())?;
    let expires_at = now_secs() + token.expires_in.unwrap_or(3600) - 60;
    let user = fetch_userinfo(&token.access_token).await.unwrap_or(UserInfo {
        email: None,
        name: None,
    });

    Ok(TokenInfo {
        access_token: token.access_token,
        refresh_token: token.refresh_token,
        expires_at,
        email: user.email,
        name: user.name,
    })
}

async fn refresh_access_token(client: &Installed, refresh_token: &str) -> Result<TokenInfo, String> {
    let http = reqwest::Client::new();
    let resp = http
        .post(&client.token_uri)
        .form(&[
            ("client_id", client.client_id.as_str()),
            ("client_secret", client.client_secret.as_str()),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Error al refrescar token: {body}"));
    }

    let token: TokenResponse = resp.json().await.map_err(|e| e.to_string())?;
    let expires_at = now_secs() + token.expires_in.unwrap_or(3600) - 60;
    let user = fetch_userinfo(&token.access_token).await.unwrap_or(UserInfo {
        email: None,
        name: None,
    });

    Ok(TokenInfo {
        access_token: token.access_token,
        refresh_token: Some(refresh_token.to_string()),
        expires_at,
        email: user.email,
        name: user.name,
    })
}

pub async fn ensure_access_token(app: &AppHandle) -> Result<TokenInfo, String> {
    let mut tokens = load_tokens(app)?.ok_or_else(|| "No hay sesión. Inicia sesión con Google.".to_string())?;
    if tokens.expires_at > now_secs() + 30 {
        return Ok(tokens);
    }
    let refresh = tokens
        .refresh_token
        .clone()
        .ok_or_else(|| "Sesión expirada y sin refresh token. Vuelve a iniciar sesión.".to_string())?;
    let prev_email = tokens.email.clone();
    let prev_name = tokens.name.clone();
    let client = load_oauth_client(app)?;
    tokens = refresh_access_token(&client, &refresh).await?;
    if tokens.refresh_token.is_none() {
        tokens.refresh_token = Some(refresh);
    }
    if tokens.email.is_none() {
        tokens.email = prev_email;
    }
    if tokens.name.is_none() {
        tokens.name = prev_name;
    }
    save_tokens(app, &tokens)?;
    Ok(tokens)
}

/// Cancela el login en curso (la espera OAuth revisa esta bandera cada ~400ms).
pub fn cancel_pending_login() -> Result<(), String> {
    OAUTH_CANCEL.store(true, Ordering::SeqCst);
    Ok(())
}

fn respond_oauth_html(request: tiny_http::Request, cancelled: bool) {
    let html = if cancelled {
        "<html><body style='font-family:Segoe UI,sans-serif;padding:2rem'><h2>Cancelado</h2><p>Puedes cerrar esta ventana.</p></body></html>"
    } else {
        "<html><body style='font-family:Segoe UI,sans-serif;padding:2rem'><h2>Listo</h2><p>Ya puedes cerrar esta ventana y volver a Tangente Chat.</p></body></html>"
    };
    let _ = request.respond(Response::from_string(html).with_header(
        "Content-Type: text/html; charset=utf-8"
            .parse::<tiny_http::Header>()
            .unwrap(),
    ));
}

fn wait_for_oauth_code_blocking(port: u16) -> Result<(String, String), String> {
    let server = Server::http(format!("{OAUTH_HOST}:{port}")).map_err(|e| {
        format!(
            "No se pudo abrir el puerto local {port}: {e}. Pulsa Cancelar e inténtalo de nuevo."
        )
    })?;

    let deadline = Instant::now() + Duration::from_secs(180);

    loop {
        if OAUTH_CANCEL.load(Ordering::SeqCst) {
            return Err("Login cancelado.".into());
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err("Tiempo de espera agotado en el login de Google (3 min)".into());
        }
        let slice = remaining.min(Duration::from_millis(400));
        match server.recv_timeout(slice) {
            Ok(Some(request)) => {
                let url = format!("http://{OAUTH_HOST}{}", request.url());
                let parsed = Url::parse(&url).ok();
                let error = parsed.as_ref().and_then(|u| {
                    u.query_pairs()
                        .find(|(k, _)| k == "error")
                        .map(|(_, v)| v.to_string())
                });
                let code = parsed.as_ref().and_then(|u| {
                    u.query_pairs()
                        .find(|(k, _)| k == "code")
                        .map(|(_, v)| v.to_string())
                });
                let state = parsed.as_ref().and_then(|u| {
                    u.query_pairs()
                        .find(|(k, _)| k == "state")
                        .map(|(_, v)| v.to_string())
                });

                if let Some(err) = error {
                    respond_oauth_html(request, true);
                    return Err(if err == "access_denied" {
                        "Login cancelado.".into()
                    } else {
                        format!("Google rechazó el login: {err}")
                    });
                }

                respond_oauth_html(request, false);
                return match code {
                    Some(code) => Ok((code, state.unwrap_or_default())),
                    None => Err("Google no devolvió authorization code".into()),
                };
            }
            Ok(None) => continue,
            Err(e) => return Err(e.to_string()),
        }
    }
}

pub async fn login_with_google(app: AppHandle) -> Result<TokenInfo, String> {
    OAUTH_CANCEL.store(false, Ordering::SeqCst);

    let client = load_oauth_client(&app)?;
    let (verifier, challenge) = pkce_pair();
    let mut state_bytes = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut state_bytes);
    let state = URL_SAFE_NO_PAD.encode(state_bytes);
    let redirect_uri = format!("http://{OAUTH_HOST}:{OAUTH_PORT}");

    let mut auth = Url::parse(&client.auth_uri).map_err(|e| e.to_string())?;
    {
        let mut q = auth.query_pairs_mut();
        q.append_pair("client_id", &client.client_id);
        q.append_pair("redirect_uri", &redirect_uri);
        q.append_pair("response_type", "code");
        q.append_pair("scope", &SCOPES.join(" "));
        q.append_pair("access_type", "offline");
        // select_account permite elegir otra cuenta al agregar; consent pide refresh token.
        q.append_pair("prompt", "select_account consent");
        q.append_pair("code_challenge", &challenge);
        q.append_pair("code_challenge_method", "S256");
        q.append_pair("state", &state);
    }

    let app_handle = app.clone();
    let auth_url = auth.to_string();
    // Arranca el loopback antes de abrir el navegador.
    let wait_task =
        tauri::async_runtime::spawn_blocking(move || wait_for_oauth_code_blocking(OAUTH_PORT));
    std::thread::sleep(Duration::from_millis(120));
    if let Err(e) = crate::browser::open_url_with_preferred_browser(&app, &auth_url) {
        OAUTH_CANCEL.store(true, Ordering::SeqCst);
        let _ = wait_task.await;
        return Err(e);
    }

    let (code, returned_state) = wait_task.await.map_err(|e| e.to_string())??;

    if returned_state != state {
        return Err("State OAuth inválido (posible CSRF)".into());
    }

    let tokens = exchange_code(&client, &code, &redirect_uri, &verifier).await?;
    let mut store = load_store(&app_handle)?;
    let saved = upsert_account(&mut store, tokens)?;
    save_store(&app_handle, &store)?;
    Ok(saved)
}
