use tauri::{image::Image, AppHandle, Manager, UserAttentionType};

const TRAY_ID: &str = "main";

fn png_to_image(bytes: &[u8]) -> Result<Image<'static>, String> {
    let img = image::load_from_memory(bytes)
        .map_err(|e| e.to_string())?
        .into_rgba8();
    let (w, h) = img.dimensions();
    Ok(Image::new_owned(img.into_raw(), w, h))
}

fn unread_tray_icon() -> Result<Image<'static>, String> {
    png_to_image(include_bytes!("../icons/tray-unread-128.png"))
}

fn overlay_badge_icon() -> Result<Image<'static>, String> {
    png_to_image(include_bytes!("../icons/overlay-badge-32.png"))
}

/// Cambia icono de bandeja + overlay de barra de tareas (estilo Teams).
pub fn set_unread_attention(app: &AppHandle, has_unread: bool) -> Result<(), String> {
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if has_unread {
            tray.set_icon(Some(unread_tray_icon()?))
                .map_err(|e| e.to_string())?;
            let _ = tray.set_tooltip(Some("Tangente Chat — mensajes nuevos"));
        } else if let Some(icon) = app.default_window_icon() {
            tray.set_icon(Some(icon.clone())).map_err(|e| e.to_string())?;
            let _ = tray.set_tooltip(Some("Tangente Chat"));
        }
    }

    if let Some(window) = app.get_webview_window("main") {
        if has_unread {
            let _ = window.set_title("Tangente Chat • nuevo");
            let _ = window.set_overlay_icon(Some(overlay_badge_icon()?));
            let _ = window.request_user_attention(Some(UserAttentionType::Informational));
        } else {
            let _ = window.set_title("Tangente Chat");
            let _ = window.set_overlay_icon(Option::<Image<'_>>::None);
            let _ = window.request_user_attention(None);
        }
    }

    Ok(())
}

pub fn tray_id() -> &'static str {
    TRAY_ID
}
