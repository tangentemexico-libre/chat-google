use crate::auth::ensure_access_token;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactItem {
    pub email: String,
    pub name: String,
    pub photo_url: Option<String>,
    pub source: String,
}

async fn people_get(app: &AppHandle, url: &str) -> Result<Value, String> {
    let tokens = ensure_access_token(app).await?;
    let client = reqwest::Client::new();
    let resp = client
        .get(url)
        .bearer_auth(&tokens.access_token)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        // Contactos/directorio pueden fallar por API no habilitada o scope; no tumbar toda la app
        return Err(format!("People API {status}: {body}"));
    }
    resp.json().await.map_err(|e| e.to_string())
}

fn push_person(map: &mut HashMap<String, ContactItem>, person: &Value, source: &str) {
    let emails = person
        .get("emailAddresses")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let display = person
        .pointer("/names/0/displayName")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let photo = person
        .pointer("/photos/0/url")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    for email_obj in emails {
        let email = email_obj
            .get("value")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if email.is_empty() || !email.contains('@') {
            continue;
        }
        let key = email.to_lowercase();
        let name = if display.is_empty() {
            email.clone()
        } else {
            display.clone()
        };
        map.entry(key).or_insert(ContactItem {
            email,
            name,
            photo_url: photo.clone(),
            source: source.to_string(),
        });
    }
}

pub async fn list_contacts(app: &AppHandle) -> Result<Vec<ContactItem>, String> {
    let me = ensure_access_token(app).await?;
    let my_email = me.email.unwrap_or_default().to_lowercase();
    let mut map: HashMap<String, ContactItem> = HashMap::new();
    let mut warnings: Vec<String> = Vec::new();

    // Directorio del dominio Workspace
    match people_get(
        app,
        "https://people.googleapis.com/v1/people:listDirectoryPeople?readMask=names,emailAddresses,photos&sources=DIRECTORY_SOURCE_TYPE_DOMAIN_PROFILE&pageSize=200",
    )
    .await
    {
        Ok(data) => {
            if let Some(people) = data.get("people").and_then(|v| v.as_array()) {
                for p in people {
                    push_person(&mut map, p, "directory");
                }
            }
        }
        Err(e) => warnings.push(e),
    }

    // Contactos de Google Contacts
    match people_get(
        app,
        "https://people.googleapis.com/v1/people/me/connections?personFields=names,emailAddresses,photos&pageSize=200",
    )
    .await
    {
        Ok(data) => {
            if let Some(people) = data.get("connections").and_then(|v| v.as_array()) {
                for p in people {
                    push_person(&mut map, p, "contacts");
                }
            }
        }
        Err(e) => warnings.push(e),
    }

    // Otros contactos (interacciones)
    match people_get(
        app,
        "https://people.googleapis.com/v1/otherContacts?readMask=names,emailAddresses&pageSize=100",
    )
    .await
    {
        Ok(data) => {
            if let Some(people) = data.get("otherContacts").and_then(|v| v.as_array()) {
                for p in people {
                    push_person(&mut map, p, "other");
                }
            }
        }
        Err(e) => warnings.push(e),
    }

    let mut items: Vec<ContactItem> = map
        .into_values()
        .filter(|c| c.email.to_lowercase() != my_email)
        .collect();
    items.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.email.to_lowercase().cmp(&b.email.to_lowercase()))
    });

    // No tumbar el modal: el usuario puede iniciar chat escribiendo el correo.
    // Si faltan scopes, el frontend muestra aviso amable.
    if items.is_empty() && !warnings.is_empty() {
        let joined = warnings.join(" ");
        if joined.contains("ACCESS_TOKEN_SCOPE_INSUFFICIENT") || joined.contains("insufficient") {
            return Err(
                "Faltan permisos de contactos en esta sesión. Cierra sesión, agrega los scopes en Google Cloud (People/Contacts/Directory) y vuelve a iniciar sesión. Mientras tanto puedes escribir el correo completo arriba."
                    .into(),
            );
        }
        return Err(format!(
            "No se pudo cargar la lista de contactos (¿People API habilitada?). Puedes escribir el correo completo arriba. Detalle: {joined}"
        ));
    }

    Ok(items)
}
