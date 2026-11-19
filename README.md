# Tangente Chat

Cliente de **escritorio** para [Google Chat](https://chat.google.com/) (Windows, Tauri + Vue).  
UI propia + Google Chat API — no es un wrapper del navegador.

Útil si quieres un chat instalable con bandeja del sistema, notificaciones, varias cuentas y sin mezclarlo con Teams u otras herramientas.

**Licencia / uso:** código abierto para que cualquiera pueda construir el suyo. Cada quien debe crear **su propio** proyecto en Google Cloud y sus propias credenciales OAuth. **No hay secretos en este repositorio.**

## Qué incluye (v0.1.7)

- Login OAuth Desktop (PKCE) con el navegador que elijas
- Espacios, DMs, envío de texto
- Nuevo chat por correo o contactos/directorio (People API)
- Pegar imagen del portapapeles y adjuntar archivos (vista con descarga autenticada)
- Varias cuentas Google (agregar / cambiar / salir por cuenta)
- Indicador de no leídos en bandeja y barra de tareas
- Cerrar la ventana → queda en bandeja (clic derecho → Salir)

## Requisitos

| Qué | Notas |
|-----|--------|
| Windows 10/11 x64 | WebView2 (Tauri lo usa; Windows lo trae o se instala) |
| [Node.js](https://nodejs.org/) LTS | `npm` |
| [Rust](https://rustup.rs/) + toolchain MSVC | `rustup` en Windows |
| Cuenta **Google Workspace** | La Chat API como cliente de usuario no está pensada para Gmail free |
| Proyecto en [Google Cloud Console](https://console.cloud.google.com/) | Tuyas las APIs y el OAuth |

Para MSI: [WiX Toolset](https://wixtoolset.org/) (Tauri lo invoca en `tauri build`).

## Setup Google Cloud (obligatorio, una vez)

Haz esto en **tu** proyecto GCP. No uses credenciales de terceros.

1. Crea o elige un proyecto.
2. Habilita:
   - **Google Chat API**
   - **People API** (contactos / directorio)
3. **Google Chat API → Configuration**  
   Pon nombre (e.g. “Mi Chat Desktop”) e icono. Google lo pide aunque autentiques como usuario. Interactive features pueden ir desactivadas si solo usas la API como cliente.
4. **OAuth consent screen**
   - Workspace solo de tu org → **Internal**
   - Usuarios fuera de tu org → **External** (en Testing, agrega test users)
5. **Credentials → Create credentials → OAuth client ID**  
   Tipo: **Desktop app**. Descarga el JSON.
6. Guárdalo como:

```text
%APPDATA%\com.tangente.chat\google-oauth.client.json
```

Plantilla (placeholders, sin secretos reales): [`docs/google-oauth.client.json.example`](docs/google-oauth.client.json.example)

7. Redirect loopback que usa la app:

```text
http://localhost:42813
```

En clientes Desktop Google suele aceptar loopback. Si el login falla por redirect, revisa que el JSON sea tipo Desktop.

### Scopes

- `openid`, `userinfo.email`, `userinfo.profile`
- `chat.spaces.readonly`, `chat.spaces.create`
- `chat.messages`, `chat.memberships.readonly`
- `chat.users.availability.readonly`
- `contacts.readonly`, `contacts.other.readonly`, `directory.readonly`

Si cambias scopes: **Salir** en la app y vuelve a iniciar sesión (consent otra vez).

Si aparece *insufficient authentication scopes* / `ACCESS_TOKEN_SCOPE_INSUFFICIENT` al listar chats:

1. En GCP → OAuth consent screen, agrega **todos** los scopes de arriba (Chat + People).
2. Si el consent es **External** y está en Testing, agrega el correo como **test user**.
3. En la app: **Salir** → iniciar sesión de nuevo y aceptar **todos** los permisos.
4. En Workspace, un admin puede tener que permitir la app / los scopes de Chat.

## Desarrollo

```powershell
git clone https://github.com/tangentemexico-libre/chat-google.git
cd chat-google
npm install
npm run tauri dev
```

La primera vez Rust descarga dependencias (puede tardar).

## Build instalador (MSI)

```powershell
npm run tauri build
```

Salida típica:

```text
src-tauri\target\release\bundle\msi\
```

El MSI de esta app **no está firmado**. Windows SmartScreen puede avisar: *Más información → Ejecutar de todas formas*. Para distribución seria hace falta certificado de firma de código (p. ej. EV).

## Después de instalar / primera ejecución

1. Copia tu `google-oauth.client.json` a `%APPDATA%\com.tangente.chat\` (la app muestra la ruta y puede abrir la carpeta).
2. Abre Tangente Chat → elige navegador para el login de Google → Iniciar sesión.
3. Multi-cuenta: clic en tu nombre (sidebar) → **Agregar cuenta**.
4. No leídos: con la ventana minimizada/oculta, actividad nueva marca bandeja + overlay en la barra de tareas; al enfocar se limpia.

## Estructura del repo

```text
src/                 Vue (UI)
src-tauri/src/       Rust (OAuth, Chat API, People, bandeja, badge)
docs/                Ejemplo OAuth (sin secretos)
```

Datos locales (no van al git):

- `%APPDATA%\com.tangente.chat\google-oauth.client.json`
- `%APPDATA%\com.tangente.chat\accounts.json` (tokens por cuenta)

## Límites honestos

- Quien corre la app como cliente de la API necesita **Workspace** con Chat. Interlocutores en Gmail free pueden fallar o tener limitaciones.
- Hoy hay **polling** (~5 s mensajes, ~30 s lista de espacios). No es push en tiempo real.
- DMs / espacios dependen de permisos de tu dominio y de cómo esté configurado Chat en GCP.
- No compartas tu `client_secret` ni `accounts.json`. Si se filtran, rota el cliente OAuth en GCP.

## Para una IA o un humano que retome el proyecto

Checklist mínimo:

1. Clonar repo → `npm install` → `npm run tauri dev`.
2. Crear proyecto GCP propio; habilitar Chat API + People API; OAuth Desktop; JSON en `%APPDATA%\com.tangente.chat\`.
3. Configurar la app de Chat en GCP (nombre/icono).
4. Probar login, listar espacios, enviar mensaje, adjunto, segunda cuenta, badge de no leídos.
5. No commitear `google-oauth.client.json`, `tokens.json`, `accounts.json`, `.env`.
6. Cambios de producto recientes: multi-cuenta (`accounts.json` + selector en sidebar) e indicador de atención (`src-tauri/src/attention.rs` + `set_unread_attention`).

## Autor

[Tangente](https://github.com/tangentemexico-libre) — Transformación Inteligente.
