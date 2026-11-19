use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserOption {
    pub id: String,
    pub name: String,
    pub path: Option<String>,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Settings {
    /// "default" | "chrome" | "edge" | "firefox" | "brave" | "custom"
    browser_id: String,
    custom_path: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            browser_id: "default".into(),
            custom_path: None,
        }
    }
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("No app data dir: {e}"))?;
    Ok(dir.join("settings.json"))
}

fn load_settings(app: &AppHandle) -> Settings {
    let Ok(path) = settings_path(app) else {
        return Settings::default();
    };
    let Ok(raw) = fs::read_to_string(path) else {
        return Settings::default();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save_settings(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

fn first_existing(paths: &[&str]) -> Option<PathBuf> {
    paths.iter().map(PathBuf::from).find(|p| p.is_file())
}

fn local_app_data() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Users\Public"))
}

fn program_files() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(p) = std::env::var_os("ProgramFiles") {
        dirs.push(PathBuf::from(p));
    }
    if let Some(p) = std::env::var_os("ProgramFiles(x86)") {
        dirs.push(PathBuf::from(p));
    }
    if dirs.is_empty() {
        dirs.push(PathBuf::from(r"C:\Program Files"));
        dirs.push(PathBuf::from(r"C:\Program Files (x86)"));
    }
    dirs
}

fn chrome_path() -> Option<PathBuf> {
    let mut candidates: Vec<String> = Vec::new();
    for pf in program_files() {
        candidates.push(
            pf.join(r"Google\Chrome\Application\chrome.exe")
                .to_string_lossy()
                .into(),
        );
    }
    candidates.push(
        local_app_data()
            .join(r"Google\Chrome\Application\chrome.exe")
            .to_string_lossy()
            .into(),
    );
    let refs: Vec<&str> = candidates.iter().map(|s| s.as_str()).collect();
    first_existing(&refs)
}

fn edge_path() -> Option<PathBuf> {
    let mut candidates: Vec<String> = Vec::new();
    for pf in program_files() {
        candidates.push(
            pf.join(r"Microsoft\Edge\Application\msedge.exe")
                .to_string_lossy()
                .into(),
        );
    }
    let refs: Vec<&str> = candidates.iter().map(|s| s.as_str()).collect();
    first_existing(&refs)
}

fn firefox_path() -> Option<PathBuf> {
    let mut candidates: Vec<String> = Vec::new();
    for pf in program_files() {
        candidates.push(
            pf.join(r"Mozilla Firefox\firefox.exe")
                .to_string_lossy()
                .into(),
        );
    }
    let refs: Vec<&str> = candidates.iter().map(|s| s.as_str()).collect();
    first_existing(&refs)
}

fn brave_path() -> Option<PathBuf> {
    let mut candidates: Vec<String> = Vec::new();
    for pf in program_files() {
        candidates.push(
            pf.join(r"BraveSoftware\Brave-Browser\Application\brave.exe")
                .to_string_lossy()
                .into(),
        );
    }
    candidates.push(
        local_app_data()
            .join(r"BraveSoftware\Brave-Browser\Application\brave.exe")
            .to_string_lossy()
            .into(),
    );
    let refs: Vec<&str> = candidates.iter().map(|s| s.as_str()).collect();
    first_existing(&refs)
}

pub fn list_browsers(app: &AppHandle) -> Vec<BrowserOption> {
    let settings = load_settings(app);
    let mut list = vec![BrowserOption {
        id: "default".into(),
        name: "Navegador predeterminado de Windows".into(),
        path: None,
        available: true,
    }];

    let chrome = chrome_path();
    list.push(BrowserOption {
        id: "chrome".into(),
        name: "Google Chrome".into(),
        path: chrome.as_ref().map(|p| p.display().to_string()),
        available: chrome.is_some(),
    });

    let edge = edge_path();
    list.push(BrowserOption {
        id: "edge".into(),
        name: "Microsoft Edge".into(),
        path: edge.as_ref().map(|p| p.display().to_string()),
        available: edge.is_some(),
    });

    let firefox = firefox_path();
    list.push(BrowserOption {
        id: "firefox".into(),
        name: "Mozilla Firefox".into(),
        path: firefox.as_ref().map(|p| p.display().to_string()),
        available: firefox.is_some(),
    });

    let brave = brave_path();
    list.push(BrowserOption {
        id: "brave".into(),
        name: "Brave".into(),
        path: brave.as_ref().map(|p| p.display().to_string()),
        available: brave.is_some(),
    });

    if let Some(custom) = settings.custom_path.clone() {
        let exists = Path::new(&custom).is_file();
        list.push(BrowserOption {
            id: "custom".into(),
            name: format!("Personalizado ({custom})"),
            path: Some(custom),
            available: exists,
        });
    }

    list
}

pub fn get_browser_preference(app: &AppHandle) -> serde_json::Value {
    let s = load_settings(app);
    serde_json::json!({
        "browser_id": s.browser_id,
        "custom_path": s.custom_path,
    })
}

pub fn set_browser_preference(
    app: &AppHandle,
    browser_id: String,
    custom_path: Option<String>,
) -> Result<(), String> {
    let id = browser_id.trim().to_lowercase();
    let allowed = ["default", "chrome", "edge", "firefox", "brave", "custom"];
    if !allowed.contains(&id.as_str()) {
        return Err("Navegador no válido".into());
    }
    if id == "custom" {
        let path = custom_path
            .as_ref()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| "Indica la ruta del .exe del navegador".to_string())?;
        if !Path::new(&path).is_file() {
            return Err(format!("No existe el ejecutable: {path}"));
        }
        save_settings(
            app,
            &Settings {
                browser_id: id,
                custom_path: Some(path),
            },
        )?;
    } else {
        // Validar que esté instalado (salvo default)
        if id != "default" {
            let browsers = list_browsers(app);
            let found = browsers.iter().find(|b| b.id == id);
            if found.map(|b| !b.available).unwrap_or(true) {
                return Err(format!("Ese navegador no está instalado en esta PC ({id})"));
            }
        }
        let mut s = load_settings(app);
        s.browser_id = id;
        save_settings(app, &s)?;
    }
    Ok(())
}

fn resolve_browser_exe(app: &AppHandle) -> Result<Option<PathBuf>, String> {
    let s = load_settings(app);
    match s.browser_id.as_str() {
        "default" => Ok(None),
        "chrome" => chrome_path().ok_or_else(|| "Chrome no encontrado".into()).map(Some),
        "edge" => edge_path().ok_or_else(|| "Edge no encontrado".into()).map(Some),
        "firefox" => firefox_path()
            .ok_or_else(|| "Firefox no encontrado".into())
            .map(Some),
        "brave" => brave_path().ok_or_else(|| "Brave no encontrado".into()).map(Some),
        "custom" => {
            let path = s
                .custom_path
                .ok_or_else(|| "No hay ruta personalizada de navegador".to_string())?;
            let p = PathBuf::from(&path);
            if !p.is_file() {
                return Err(format!("No existe: {path}"));
            }
            Ok(Some(p))
        }
        other => Err(format!("Preferencia de navegador desconocida: {other}")),
    }
}

pub fn open_url_with_preferred_browser(app: &AppHandle, url: &str) -> Result<(), String> {
    match resolve_browser_exe(app)? {
        None => tauri_plugin_opener::open_url(url, None::<&str>)
            .map_err(|e| format!("No se pudo abrir el navegador predeterminado: {e}")),
        Some(exe) => {
            Command::new(&exe)
                .arg(url)
                .spawn()
                .map_err(|e| format!("No se pudo abrir {}: {e}", exe.display()))?;
            Ok(())
        }
    }
}
