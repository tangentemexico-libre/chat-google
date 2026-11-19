use crate::auth::{ensure_access_token, TokenInfo};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpaceItem {
    pub name: String,
    pub display_name: String,
    pub space_type: String,
    pub last_active_time: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageItem {
    pub name: String,
    pub text: String,
    pub sender_name: String,
    pub sender_email: String,
    pub create_time: String,
    pub mine: bool,
    /// Nombre del archivo adjunto (si hay).
    pub attachment_name: Option<String>,
    /// MIME del adjunto (image/png, application/pdf, …).
    pub attachment_mime: Option<String>,
    /// resourceName para media.download (OAuth); las URLs públicas de Chat no sirven en la app.
    pub attachment_resource: Option<String>,
}

fn format_chat_api_error(status: reqwest::StatusCode, body: &str) -> String {
    let lower = body.to_ascii_lowercase();
    if lower.contains("access_token_scope_insufficient")
        || lower.contains("insufficient authentication scopes")
    {
        return "Faltan permisos de Google Chat en esta sesión. Pulsa Salir, vuelve a iniciar sesión y acepta todos los permisos (espacios y mensajes). En Google Cloud, OAuth consent screen debe incluir los scopes de Chat; si el proyecto es External/Testing, agrega al usuario como test user.".into();
    }
    if status.as_u16() == 403 && lower.contains("permission_denied") {
        return format!(
            "Google Chat denegó el acceso (403). Revisa que la cuenta tenga Google Chat y que la app esté configurada en GCP. Detalle: {body}"
        );
    }
    format!("Chat API {status}: {body}")
}

async fn chat_get(app: &AppHandle, path: &str) -> Result<Value, String> {
    let tokens = ensure_access_token(app).await?;
    let url = format!("https://chat.googleapis.com/v1{path}");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .bearer_auth(&tokens.access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format_chat_api_error(status, &body));
    }
    resp.json().await.map_err(|e| e.to_string())
}

async fn chat_post(app: &AppHandle, path: &str, body: Value) -> Result<Value, String> {
    let tokens = ensure_access_token(app).await?;
    let url = format!("https://chat.googleapis.com/v1{path}");
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(&tokens.access_token)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format_chat_api_error(status, &body));
    }
    resp.json().await.map_err(|e| e.to_string())
}

fn space_label(space: &Value) -> String {
    if let Some(dn) = space.get("displayName").and_then(|v| v.as_str()) {
        if !dn.trim().is_empty() {
            return dn.to_string();
        }
    }
    let space_type = space
        .get("spaceType")
        .and_then(|v| v.as_str())
        .unwrap_or("SPACE");
    match space_type {
        "DIRECT_MESSAGE" => "Chat directo".into(),
        "GROUP_CHAT" => "Grupo".into(),
        _ => "Espacio".into(),
    }
}

pub async fn list_spaces(app: &AppHandle) -> Result<Vec<SpaceItem>, String> {
    let filter = "spaceType = \"SPACE\" OR spaceType = \"GROUP_CHAT\" OR spaceType = \"DIRECT_MESSAGE\"";
    let encoded = urlencoding_lite(filter);
    let data = chat_get(
        app,
        &format!("/spaces?pageSize=100&filter={encoded}"),
    )
    .await?;

    let mut items = Vec::new();
    if let Some(spaces) = data.get("spaces").and_then(|v| v.as_array()) {
        for space in spaces {
            let name = space
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            if name.is_empty() {
                continue;
            }
            items.push(SpaceItem {
                name: name.clone(),
                display_name: space_label(space),
                space_type: space
                    .get("spaceType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("SPACE")
                    .to_string(),
                last_active_time: space
                    .get("lastActiveTime")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            });
        }
    }

    // Enriquecer DMs / grupos sin displayName con nombres de miembros
    for item in items.iter_mut() {
        if item.display_name == "Chat directo" || item.display_name == "Grupo" {
            if let Ok(label) = members_label(app, &item.name).await {
                if !label.is_empty() {
                    item.display_name = label;
                }
            }
        }
    }

    items.sort_by(|a, b| b.last_active_time.cmp(&a.last_active_time));
    Ok(items)
}

async fn members_label(app: &AppHandle, space_name: &str) -> Result<String, String> {
    let me = ensure_access_token(app).await?;
    let data = chat_get(app, &format!("/{space_name}/members?pageSize=20")).await?;
    let mut names = Vec::new();
    if let Some(members) = data.get("memberships").and_then(|v| v.as_array()) {
        for m in members {
            let email = m
                .pointer("/member/email")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let display = m
                .pointer("/member/displayName")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if let Some(my_email) = me.email.as_deref() {
                if !email.is_empty() && email.eq_ignore_ascii_case(my_email) {
                    continue;
                }
            }
            if !display.is_empty() {
                names.push(display.to_string());
            } else if !email.is_empty() {
                names.push(email.to_string());
            }
        }
    }
    Ok(names.join(", "))
}

pub async fn list_messages(app: &AppHandle, space_name: String) -> Result<Vec<MessageItem>, String> {
    let me = ensure_access_token(app).await?;
    let path = format!("/{space_name}/messages?pageSize=50&orderBy=createTime desc");
    let data = chat_get(app, &path).await?;
    let mut items = Vec::new();
    if let Some(messages) = data.get("messages").and_then(|v| v.as_array()) {
        for msg in messages {
            items.push(map_message(msg, &me));
        }
    }
    items.reverse();
    Ok(items)
}

fn map_message(msg: &Value, me: &TokenInfo) -> MessageItem {
    let sender_email = msg
        .pointer("/sender/email")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let sender_name = msg
        .pointer("/sender/displayName")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            if sender_email.is_empty() {
                "Desconocido"
            } else {
                sender_email.as_str()
            }
        })
        .to_string();
    let mine = me
        .email
        .as_ref()
        .map(|e| e.eq_ignore_ascii_case(&sender_email))
        .unwrap_or(false);

    let (attachment_name, attachment_mime, attachment_resource) = first_attachment(msg);

    MessageItem {
        name: msg
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        text: msg
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        sender_name,
        sender_email,
        create_time: msg
            .get("createTime")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        mine,
        attachment_name,
        attachment_mime,
        attachment_resource,
    }
}

fn first_attachment(msg: &Value) -> (Option<String>, Option<String>, Option<String>) {
    let Some(attachments) = msg.get("attachment").and_then(|v| v.as_array()) else {
        return (None, None, None);
    };
    for att in attachments {
        let name = att
            .get("contentName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let mime = att
            .get("contentType")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let resource = att
            .pointer("/attachmentDataRef/resourceName")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        if name.is_some() || resource.is_some() {
            return (name, mime, resource);
        }
    }
    (None, None, None)
}

/// Descarga un adjunto con OAuth y lo devuelve como data URL (para mostrar en la UI).
pub async fn download_attachment_data_url(
    app: &AppHandle,
    resource_name: String,
    mime_hint: Option<String>,
) -> Result<String, String> {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let resource_name = resource_name.trim();
    if resource_name.is_empty() {
        return Err("Adjunto sin resourceName".into());
    }

    let tokens = ensure_access_token(app).await?;
    // resourceName debe ir tal cual (token opaco); no decodificar ni alterar.
    let url = format!("https://chat.googleapis.com/v1/media/{resource_name}?alt=media");
    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .bearer_auth(&tokens.access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Chat media download {status}: {body}"));
    }

    let header_mime = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).trim().to_string());
    let mime = header_mime
        .or(mime_hint)
        .unwrap_or_else(|| "application/octet-stream".into());
    let bytes = resp.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > 25 * 1024 * 1024 {
        return Err("El adjunto es demasiado grande para previsualizar (>25MB)".into());
    }
    Ok(format!(
        "data:{mime};base64,{}",
        STANDARD.encode(bytes)
    ))
}

pub async fn send_message(
    app: &AppHandle,
    space_name: String,
    text: String,
) -> Result<MessageItem, String> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err("El mensaje está vacío".into());
    }
    let me = ensure_access_token(app).await?;
    let body = serde_json::json!({ "text": text });
    let data = chat_post(app, &format!("/{space_name}/messages"), body).await?;
    Ok(map_message(&data, &me))
}

fn build_multipart_related(filename: &str, mime_type: &str, bytes: &[u8]) -> (String, Vec<u8>) {
    let boundary = format!(
        "tangente_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let meta = serde_json::json!({ "filename": filename }).to_string();
    let mut body: Vec<u8> = Vec::with_capacity(bytes.len() + 512);
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(b"Content-Type: application/json; charset=UTF-8\r\n\r\n");
    body.extend_from_slice(meta.as_bytes());
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(format!("Content-Type: {mime_type}\r\n\r\n").as_bytes());
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    (
        format!("multipart/related; boundary={boundary}"),
        body,
    )
}

async fn upload_attachment(
    app: &AppHandle,
    space_name: &str,
    filename: &str,
    mime_type: &str,
    bytes: &[u8],
) -> Result<Value, String> {
    if bytes.is_empty() {
        return Err("La imagen está vacía".into());
    }
    // Límite práctico local (API admite hasta 200MB; portapapeles no debería acercarse)
    if bytes.len() > 20 * 1024 * 1024 {
        return Err("La imagen supera 20 MB".into());
    }

    let tokens = ensure_access_token(app).await?;
    let (content_type, body) = build_multipart_related(filename, mime_type, bytes);
    let url = format!(
        "https://chat.googleapis.com/upload/v1/{space_name}/attachments:upload?uploadType=multipart"
    );
    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .bearer_auth(&tokens.access_token)
        .header("Content-Type", content_type)
        .body(body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("Chat upload {status}: {body}"));
    }
    resp.json().await.map_err(|e| e.to_string())
}

/// Sube archivo (imagen u otro permitido por Chat) y lo envía como mensaje adjunto.
pub async fn send_file_message(
    app: &AppHandle,
    space_name: String,
    filename: String,
    mime_type: String,
    data_base64: String,
    caption: Option<String>,
) -> Result<MessageItem, String> {
    use base64::{engine::general_purpose::STANDARD, Engine};

    let mime_type = if mime_type.trim().is_empty() {
        "application/octet-stream".to_string()
    } else {
        mime_type.trim().to_string()
    };

    let filename = {
        let f = filename.trim();
        if f.is_empty() {
            let ext = mime_type.split('/').nth(1).unwrap_or("bin");
            format!("archivo.{ext}")
        } else {
            f.to_string()
        }
    };

    let raw = data_base64.trim();
    let b64 = raw
        .strip_prefix("data:")
        .and_then(|s| s.split(',').nth(1))
        .unwrap_or(raw);
    let bytes = STANDARD
        .decode(b64)
        .map_err(|e| format!("Base64 inválido: {e}"))?;

    let uploaded = upload_attachment(app, &space_name, &filename, &mime_type, &bytes).await?;
    let me = ensure_access_token(app).await?;

    let caption = caption.unwrap_or_default().trim().to_string();
    let mut body = serde_json::json!({
        "attachment": [uploaded]
    });
    if !caption.is_empty() {
        body["text"] = Value::String(caption);
    }

    let data = chat_post(app, &format!("/{space_name}/messages"), body).await?;
    Ok(map_message(&data, &me))
}

/// Compat: alias para pegar imagen del portapapeles.
pub async fn send_image_message(
    app: &AppHandle,
    space_name: String,
    filename: String,
    mime_type: String,
    data_base64: String,
    caption: Option<String>,
) -> Result<MessageItem, String> {
    let mime = if mime_type.trim().is_empty() {
        "image/png".into()
    } else {
        mime_type
    };
    if !mime.starts_with("image/") {
        return Err("El portapapeles no contiene una imagen".into());
    }
    send_file_message(app, space_name, filename, mime, data_base64, caption).await
}

fn is_valid_email(email: &str) -> bool {
    let email = email.trim();
    let mut parts = email.split('@');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(local), Some(domain), None) => {
            !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
        }
        _ => false,
    }
}

/// Busca DM existente o crea uno nuevo con el correo indicado.
pub async fn start_direct_chat(app: &AppHandle, email: String) -> Result<SpaceItem, String> {
    let email = email.trim().to_lowercase();
    if !is_valid_email(&email) {
        return Err("Escribe un correo válido (ej. alguien@empresa.com)".into());
    }

    let me = ensure_access_token(app).await?;
    if me
        .email
        .as_ref()
        .map(|e| e.eq_ignore_ascii_case(&email))
        .unwrap_or(false)
    {
        return Err("No puedes iniciar un chat contigo mismo".into());
    }

    let user_resource = format!("users/{email}");

    // 1) ¿Ya existe el DM?
    let find_path = format!(
        "/spaces:findDirectMessage?name={}",
        urlencoding_lite(&user_resource)
    );
    if let Ok(existing) = chat_get(app, &find_path).await {
        if let Some(name) = existing.get("name").and_then(|v| v.as_str()) {
            if !name.is_empty() {
                return Ok(SpaceItem {
                    name: name.to_string(),
                    display_name: email.clone(),
                    space_type: existing
                        .get("spaceType")
                        .and_then(|v| v.as_str())
                        .unwrap_or("DIRECT_MESSAGE")
                        .to_string(),
                    last_active_time: existing
                        .get("lastActiveTime")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                });
            }
        }
    }

    // 2) Crear DM
    let body = serde_json::json!({
        "space": { "spaceType": "DIRECT_MESSAGE" },
        "memberships": [{
            "member": {
                "name": user_resource,
                "type": "HUMAN"
            }
        }]
    });

    let created = chat_post(app, "/spaces:setup", body).await.map_err(|e| {
        format!(
            "No se pudo iniciar chat con {email}. Si es Gmail externo puede estar bloqueado por la API; prueba con correo Workspace. Detalle: {e}"
        )
    })?;

    let name = created
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Google no devolvió el espacio del nuevo chat".to_string())?
        .to_string();

    Ok(SpaceItem {
        name,
        display_name: email,
        space_type: "DIRECT_MESSAGE".into(),
        last_active_time: None,
    })
}

fn urlencoding_lite(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for b in input.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
