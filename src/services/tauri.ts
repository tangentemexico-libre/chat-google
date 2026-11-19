import { invoke } from "@tauri-apps/api/core";

export type TokenInfo = {
  access_token: string;
  refresh_token?: string | null;
  expires_at: number;
  email?: string | null;
  name?: string | null;
};

export type AccountSummary = {
  email: string;
  name?: string | null;
  active: boolean;
};

export type SpaceItem = {
  name: string;
  display_name: string;
  space_type: string;
  last_active_time?: string | null;
};

export type MessageItem = {
  name: string;
  text: string;
  sender_name: string;
  sender_email: string;
  create_time: string;
  mine: boolean;
  attachment_name?: string | null;
  attachment_mime?: string | null;
  attachment_resource?: string | null;
};

export type CredentialsStatus = {
  path: string;
  exists: boolean;
  redirect_uri: string;
};

export type ContactItem = {
  email: string;
  name: string;
  photo_url?: string | null;
  source: string;
};

export function authStatus() {
  return invoke<TokenInfo | null>("auth_status");
}

export function credentialsStatus() {
  return invoke<CredentialsStatus>("credentials_status");
}

export function loginGoogle() {
  return invoke<TokenInfo>("login_google");
}

/** Cancela un login OAuth en espera (libera el puerto y desbloquea la UI). */
export function cancelLogin() {
  return invoke<void>("cancel_login");
}

export function listAccounts() {
  return invoke<AccountSummary[]>("list_accounts");
}

export function switchAccount(email: string) {
  return invoke<TokenInfo>("switch_account", { email });
}

export function removeAccount(email: string) {
  return invoke<TokenInfo | null>("remove_account", { email });
}

/** Cierra la cuenta activa. Si quedan otras, devuelve la siguiente. */
export function logout() {
  return invoke<TokenInfo | null>("logout");
}

export function listSpaces() {
  return invoke<SpaceItem[]>("list_spaces");
}

export function listMessages(spaceName: string) {
  return invoke<MessageItem[]>("list_messages", { spaceName });
}

export function sendMessage(spaceName: string, text: string) {
  return invoke<MessageItem>("send_message", { spaceName, text });
}

export function sendImageMessage(
  spaceName: string,
  filename: string,
  mimeType: string,
  dataBase64: string,
  caption?: string,
) {
  return invoke<MessageItem>("send_image_message", {
    spaceName,
    filename,
    mimeType,
    dataBase64,
    caption: caption ?? null,
  });
}

export function sendFileMessage(
  spaceName: string,
  filename: string,
  mimeType: string,
  dataBase64: string,
  caption?: string,
) {
  return invoke<MessageItem>("send_file_message", {
    spaceName,
    filename,
    mimeType,
    dataBase64,
    caption: caption ?? null,
  });
}

export function downloadAttachment(resourceName: string, mimeHint?: string) {
  return invoke<string>("download_attachment", {
    resourceName,
    mimeHint: mimeHint ?? null,
  });
}

export function openDataDir() {
  return invoke<void>("open_data_dir");
}

export function listContacts() {
  return invoke<ContactItem[]>("list_contacts");
}

export function startDirectChat(email: string) {
  return invoke<SpaceItem>("start_direct_chat", { email });
}

export type BrowserOption = {
  id: string;
  name: string;
  path?: string | null;
  available: boolean;
};

export function listBrowsers() {
  return invoke<BrowserOption[]>("list_browsers");
}

export function getBrowserPreference() {
  return invoke<{ browser_id: string; custom_path?: string | null }>(
    "get_browser_preference",
  );
}

export function setBrowserPreference(browserId: string, customPath?: string | null) {
  return invoke<void>("set_browser_preference", {
    browserId,
    customPath: customPath ?? null,
  });
}

/** Icono de bandeja + overlay en barra de tareas cuando hay no leídos. */
export function setUnreadAttention(hasUnread: boolean) {
  return invoke<void>("set_unread_attention", { hasUnread });
}
