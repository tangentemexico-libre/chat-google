mod attention;
mod auth;
mod browser;
mod chat_api;
mod contacts;

use auth::{AccountSummary, TokenInfo};
use chat_api::{MessageItem, SpaceItem};
use contacts::ContactItem;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, WindowEvent,
};
use tauri_plugin_autostart::MacosLauncher;

#[tauri::command]
async fn auth_status(app: tauri::AppHandle) -> Result<Option<TokenInfo>, String> {
    auth::load_tokens(&app)
}

#[tauri::command]
async fn credentials_status(app: tauri::AppHandle) -> Result<serde_json::Value, String> {
    auth::credentials_status(&app)
}

#[tauri::command]
async fn login_google(app: tauri::AppHandle) -> Result<TokenInfo, String> {
    auth::login_with_google(app).await
}

#[tauri::command]
fn cancel_login() -> Result<(), String> {
    auth::cancel_pending_login()
}

#[tauri::command]
fn list_browsers(app: tauri::AppHandle) -> Vec<browser::BrowserOption> {
    browser::list_browsers(&app)
}

#[tauri::command]
fn get_browser_preference(app: tauri::AppHandle) -> serde_json::Value {
    browser::get_browser_preference(&app)
}

#[tauri::command]
fn set_browser_preference(
    app: tauri::AppHandle,
    browser_id: String,
    custom_path: Option<String>,
) -> Result<(), String> {
    browser::set_browser_preference(&app, browser_id, custom_path)
}

#[tauri::command]
async fn list_accounts(app: tauri::AppHandle) -> Result<Vec<AccountSummary>, String> {
    auth::list_accounts(&app)
}

#[tauri::command]
async fn switch_account(app: tauri::AppHandle, email: String) -> Result<TokenInfo, String> {
    auth::switch_account(&app, email)
}

#[tauri::command]
async fn remove_account(app: tauri::AppHandle, email: String) -> Result<Option<TokenInfo>, String> {
    auth::remove_account(&app, email)
}

#[tauri::command]
async fn logout(app: tauri::AppHandle) -> Result<Option<TokenInfo>, String> {
    auth::clear_tokens(&app)
}

#[tauri::command]
async fn list_spaces(app: tauri::AppHandle) -> Result<Vec<SpaceItem>, String> {
    chat_api::list_spaces(&app).await
}

#[tauri::command]
async fn list_messages(app: tauri::AppHandle, space_name: String) -> Result<Vec<MessageItem>, String> {
    chat_api::list_messages(&app, space_name).await
}

#[tauri::command]
async fn send_message(
    app: tauri::AppHandle,
    space_name: String,
    text: String,
) -> Result<MessageItem, String> {
    chat_api::send_message(&app, space_name, text).await
}

#[tauri::command]
async fn send_image_message(
    app: tauri::AppHandle,
    space_name: String,
    filename: String,
    mime_type: String,
    data_base64: String,
    caption: Option<String>,
) -> Result<MessageItem, String> {
    chat_api::send_image_message(
        &app,
        space_name,
        filename,
        mime_type,
        data_base64,
        caption,
    )
    .await
}

#[tauri::command]
async fn send_file_message(
    app: tauri::AppHandle,
    space_name: String,
    filename: String,
    mime_type: String,
    data_base64: String,
    caption: Option<String>,
) -> Result<MessageItem, String> {
    chat_api::send_file_message(
        &app,
        space_name,
        filename,
        mime_type,
        data_base64,
        caption,
    )
    .await
}

#[tauri::command]
async fn download_attachment(
    app: tauri::AppHandle,
    resource_name: String,
    mime_hint: Option<String>,
) -> Result<String, String> {
    chat_api::download_attachment_data_url(&app, resource_name, mime_hint).await
}

#[tauri::command]
async fn list_contacts(app: tauri::AppHandle) -> Result<Vec<ContactItem>, String> {
    contacts::list_contacts(&app).await
}

#[tauri::command]
async fn start_direct_chat(app: tauri::AppHandle, email: String) -> Result<SpaceItem, String> {
    chat_api::start_direct_chat(&app, email).await
}

#[tauri::command]
fn open_data_dir(app: tauri::AppHandle) -> Result<(), String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    tauri_plugin_opener::open_path(dir, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_unread_attention(app: tauri::AppHandle, has_unread: bool) -> Result<(), String> {
    attention::set_unread_attention(&app, has_unread)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--minimized"]),
        ))
        .setup(|app| {
            let show_i = MenuItem::with_id(app, "show", "Mostrar Tangente Chat", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Salir", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

            let _tray = TrayIconBuilder::with_id(attention::tray_id())
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .tooltip("Tangente Chat")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                        let _ = attention::set_unread_attention(app, false);
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                        let _ = attention::set_unread_attention(app, false);
                    }
                })
                .build(app)?;

            if let Some(window) = app.get_webview_window("main") {
                let window_clone = window.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window_clone.hide();
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            auth_status,
            credentials_status,
            login_google,
            cancel_login,
            list_accounts,
            switch_account,
            remove_account,
            list_browsers,
            get_browser_preference,
            set_browser_preference,
            logout,
            list_spaces,
            list_messages,
            send_message,
            send_image_message,
            send_file_message,
            download_attachment,
            list_contacts,
            start_direct_chat,
            open_data_dir,
            set_unread_attention
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
