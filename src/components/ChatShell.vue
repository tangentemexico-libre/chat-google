<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import MessageAttachment from "./MessageAttachment.vue";
import NewChatModal from "./NewChatModal.vue";
import {
  cancelLogin,
  getBrowserPreference,
  listAccounts,
  listBrowsers,
  listMessages,
  listSpaces,
  loginGoogle,
  logout,
  removeAccount,
  sendFileMessage,
  sendMessage,
  setBrowserPreference,
  setUnreadAttention,
  switchAccount,
  type AccountSummary,
  type BrowserOption,
  type MessageItem,
  type SpaceItem,
  type TokenInfo,
} from "../services/tauri";

type PendingFile = {
  filename: string;
  mimeType: string;
  dataBase64: string;
  previewUrl: string | null;
  isImage: boolean;
};

const props = defineProps<{
  session: TokenInfo;
}>();

const emit = defineEmits<{
  loggedOut: [];
  sessionChanged: [TokenInfo];
}>();

const spaces = ref<SpaceItem[]>([]);
const filter = ref("");
const selected = ref<SpaceItem | null>(null);
const messages = ref<MessageItem[]>([]);
const draft = ref("");
const pendingFile = ref<PendingFile | null>(null);
const fileInput = ref<HTMLInputElement | null>(null);
const loadingSpaces = ref(false);
const loadingMessages = ref(false);
const sending = ref(false);
const error = ref("");
const messagesEl = ref<HTMLElement | null>(null);
const knownMessageNames = ref<Set<string>>(new Set());
const showNewChat = ref(false);
const accounts = ref<AccountSummary[]>([]);
const showAccounts = ref(false);
const accountBusy = ref(false);
/** Login OAuth en curso: UI espera Google; se puede cancelar. */
const loginWaiting = ref(false);
const browsers = ref<BrowserOption[]>([]);
const browserId = ref("default");
const windowFocused = ref(true);
const knownActivity = ref<Map<string, string>>(new Map());
const activitySeeded = ref(false);

let spacesTimer: number | undefined;
let messagesTimer: number | undefined;
let unlistenFocus: (() => void) | undefined;

async function markUnread() {
  try {
    await setUnreadAttention(true);
  } catch {
    /* ignore */
  }
}

async function clearUnread() {
  try {
    await setUnreadAttention(false);
  } catch {
    /* ignore */
  }
}

const filteredSpaces = computed(() => {
  const q = filter.value.trim().toLowerCase();
  if (!q) return spaces.value;
  return spaces.value.filter((s) => s.display_name.toLowerCase().includes(q));
});

const canSend = computed(() => {
  if (sending.value || !selected.value) return false;
  if (pendingFile.value) return true;
  return !!draft.value.trim();
});

function spaceTypeLabel(t: string) {
  if (t === "DIRECT_MESSAGE") return "Directo";
  if (t === "GROUP_CHAT") return "Grupo";
  return "Espacio";
}

function formatTime(iso: string) {
  if (!iso) return "";
  try {
    return new Date(iso).toLocaleString("es-MX", {
      hour: "2-digit",
      minute: "2-digit",
      day: "2-digit",
      month: "short",
    });
  } catch {
    return iso;
  }
}

function clearPendingFile() {
  if (pendingFile.value?.previewUrl) {
    URL.revokeObjectURL(pendingFile.value.previewUrl);
  }
  pendingFile.value = null;
}

async function fileToPending(file: File): Promise<PendingFile> {
  const buf = await file.arrayBuffer();
  const bytes = new Uint8Array(buf);
  let binary = "";
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  const dataBase64 = btoa(binary);
  const mimeType = file.type || "application/octet-stream";
  const isImage = mimeType.startsWith("image/");
  const ext = mimeType.split("/")[1] || "bin";
  const filename = file.name || `archivo.${ext}`;
  return {
    filename,
    mimeType,
    dataBase64,
    previewUrl: isImage ? URL.createObjectURL(file) : null,
    isImage,
  };
}

async function setPendingFromFile(file: File) {
  clearPendingFile();
  pendingFile.value = await fileToPending(file);
}

async function ensureNotifyPermission() {
  let granted = await isPermissionGranted();
  if (!granted) {
    const permission = await requestPermission();
    granted = permission === "granted";
  }
  return granted;
}

async function refreshSpaces() {
  loadingSpaces.value = true;
  error.value = "";
  try {
    spaces.value = await listSpaces();
    if (selected.value) {
      const still = spaces.value.find((s) => s.name === selected.value?.name);
      if (!still) selected.value = null;
      else selected.value = still;
    }

    let foundUnread = false;
    if (activitySeeded.value) {
      for (const space of spaces.value) {
        const next = space.last_active_time || "";
        const prev = knownActivity.value.get(space.name) || "";
        if (next && prev && next > prev) {
          const viewingHere =
            windowFocused.value && selected.value?.name === space.name;
          if (!viewingHere) foundUnread = true;
        }
      }
    }
    knownActivity.value = new Map(
      spaces.value.map((s) => [s.name, s.last_active_time || ""]),
    );
    activitySeeded.value = true;
    if (foundUnread) await markUnread();
  } catch (e) {
    error.value = String(e);
  } finally {
    loadingSpaces.value = false;
  }
}

async function refreshMessages(notifyNew: boolean) {
  if (!selected.value) return;
  loadingMessages.value = !messages.value.length;
  try {
    const list = await listMessages(selected.value.name);
    if (notifyNew && knownMessageNames.value.size) {
      const incoming = list.filter(
        (m) =>
          !m.mine &&
          !knownMessageNames.value.has(m.name) &&
          (m.text || m.attachment_resource || m.attachment_name),
      );
      if (incoming.length) {
        const last = incoming[incoming.length - 1];
        if (await ensureNotifyPermission()) {
          sendNotification({
            title: selected.value.display_name,
            body: last.text
              ? `${last.sender_name}: ${last.text.slice(0, 120)}`
              : `${last.sender_name}: [${last.attachment_name || "archivo"}]`,
          });
        }
        if (!windowFocused.value) await markUnread();
      }
    }
    knownMessageNames.value = new Set(list.map((m) => m.name));
    messages.value = list;
    await nextTick();
    if (messagesEl.value) {
      messagesEl.value.scrollTop = messagesEl.value.scrollHeight;
    }
  } catch (e) {
    error.value = String(e);
  } finally {
    loadingMessages.value = false;
  }
}

async function selectSpace(space: SpaceItem) {
  selected.value = space;
  messages.value = [];
  knownMessageNames.value = new Set();
  draft.value = "";
  clearPendingFile();
  await refreshMessages(false);
}

async function onSend() {
  if (!selected.value || sending.value) return;
  if (!pendingFile.value && !draft.value.trim()) return;

  sending.value = true;
  error.value = "";
  try {
    let msg: MessageItem;
    if (pendingFile.value) {
      msg = await sendFileMessage(
        selected.value.name,
        pendingFile.value.filename,
        pendingFile.value.mimeType,
        pendingFile.value.dataBase64,
        draft.value.trim() || undefined,
      );
      clearPendingFile();
      draft.value = "";
    } else {
      msg = await sendMessage(selected.value.name, draft.value);
      draft.value = "";
    }
    messages.value = [...messages.value, msg];
    knownMessageNames.value.add(msg.name);
    await nextTick();
    if (messagesEl.value) {
      messagesEl.value.scrollTop = messagesEl.value.scrollHeight;
    }
  } catch (e) {
    error.value = String(e);
  } finally {
    sending.value = false;
  }
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "Enter" && !e.shiftKey) {
    e.preventDefault();
    void onSend();
  }
}

async function onPaste(e: ClipboardEvent) {
  const items = e.clipboardData?.items;
  if (!items) return;
  for (const item of Array.from(items)) {
    if (item.type.startsWith("image/")) {
      e.preventDefault();
      const file = item.getAsFile();
      if (file) {
        try {
          await setPendingFromFile(file);
        } catch (err) {
          error.value = String(err);
        }
      }
      return;
    }
  }
}

async function pasteFromClipboardButton() {
  error.value = "";
  try {
    if (!navigator.clipboard?.read) {
      error.value =
        "Este WebView no permite leer el portapapeles. Usa Ctrl+V en el cuadro de mensaje.";
      return;
    }
    const items = await navigator.clipboard.read();
    for (const item of items) {
      const type = item.types.find((t) => t.startsWith("image/"));
      if (!type) continue;
      const blob = await item.getType(type);
      const file = new File([blob], `clipboard.${type.split("/")[1] || "png"}`, {
        type,
      });
      await setPendingFromFile(file);
      return;
    }
    error.value = "No hay imagen en el portapapeles.";
  } catch (e) {
    error.value = `No se pudo leer el portapapeles: ${String(e)}. Prueba Ctrl+V.`;
  }
}

function openFilePicker() {
  fileInput.value?.click();
}

async function onFilePicked(e: Event) {
  const input = e.target as HTMLInputElement;
  const file = input.files?.[0];
  input.value = "";
  if (!file) return;
  error.value = "";
  try {
    if (file.size > 20 * 1024 * 1024) {
      error.value = "El archivo supera 20 MB.";
      return;
    }
    await setPendingFromFile(file);
  } catch (err) {
    error.value = String(err);
  }
}

async function refreshAccounts() {
  try {
    accounts.value = await listAccounts();
  } catch {
    accounts.value = [];
  }
}

async function onLogout() {
  error.value = "";
  accountBusy.value = true;
  try {
    const next = await logout();
    if (next) emit("sessionChanged", next);
    else emit("loggedOut");
  } catch (e) {
    error.value = String(e);
  } finally {
    accountBusy.value = false;
  }
}

async function onSwitchAccount(email: string) {
  if (!email || email === props.session.email?.toLowerCase()) {
    showAccounts.value = false;
    return;
  }
  error.value = "";
  accountBusy.value = true;
  try {
    const token = await switchAccount(email);
    showAccounts.value = false;
    emit("sessionChanged", token);
  } catch (e) {
    error.value = String(e);
  } finally {
    accountBusy.value = false;
  }
}

async function loadBrowserOptions() {
  try {
    browsers.value = await listBrowsers();
    const pref = await getBrowserPreference();
    browserId.value = pref.browser_id || "default";
  } catch {
    browsers.value = [];
  }
}

async function onBrowserChange() {
  try {
    await setBrowserPreference(browserId.value);
  } catch (e) {
    error.value = String(e);
  }
}

async function onCancelLogin() {
  try {
    await cancelLogin();
  } catch {
    /* ignore */
  }
}

async function onAddAccount() {
  error.value = "";
  showAccounts.value = true;
  accountBusy.value = true;
  loginWaiting.value = true;
  try {
    await setBrowserPreference(browserId.value);
    const token = await loginGoogle();
    showAccounts.value = false;
    emit("sessionChanged", token);
  } catch (e) {
    const msg = String(e);
    if (!msg.toLowerCase().includes("cancelado")) {
      error.value = msg;
    }
  } finally {
    loginWaiting.value = false;
    accountBusy.value = false;
  }
}

async function onRemoveAccount(email: string) {
  error.value = "";
  accountBusy.value = true;
  try {
    const next = await removeAccount(email);
    await refreshAccounts();
    if (!next) {
      emit("loggedOut");
      return;
    }
    if (email.toLowerCase() === (props.session.email || "").toLowerCase()) {
      emit("sessionChanged", next);
    }
  } catch (e) {
    error.value = String(e);
  } finally {
    accountBusy.value = false;
  }
}

async function onChatStarted(space: SpaceItem) {
  showNewChat.value = false;
  const existing = spaces.value.find((s) => s.name === space.name);
  if (!existing) {
    spaces.value = [space, ...spaces.value];
  }
  await selectSpace(existing ?? space);
  await refreshSpaces();
  const refreshed = spaces.value.find((s) => s.name === space.name);
  if (refreshed) {
    selected.value = refreshed;
  }
}

watch(selected, (space) => {
  if (messagesTimer) window.clearInterval(messagesTimer);
  if (!space) return;
  messagesTimer = window.setInterval(() => {
    void refreshMessages(true);
  }, 5000);
});

onMounted(async () => {
  const win = getCurrentWindow();
  try {
    windowFocused.value = await win.isFocused();
    unlistenFocus = await win.onFocusChanged(({ payload: focused }) => {
      windowFocused.value = focused;
      if (focused) void clearUnread();
    });
  } catch {
    windowFocused.value = true;
  }

  await Promise.all([refreshSpaces(), refreshAccounts(), loadBrowserOptions()]);
  spacesTimer = window.setInterval(() => {
    void refreshSpaces();
  }, 30000);
});

onUnmounted(() => {
  if (spacesTimer) window.clearInterval(spacesTimer);
  if (messagesTimer) window.clearInterval(messagesTimer);
  unlistenFocus?.();
  clearPendingFile();
  void clearUnread();
  if (loginWaiting.value) void cancelLogin();
});
</script>

<template>
  <div class="tc-shell">
    <aside class="tc-sidebar">
      <div class="tc-sidebar-header">
        <div class="tc-account-trigger">
          <h2>Tangente Chat</h2>
          <button
            class="tc-user-btn"
            type="button"
            :disabled="accountBusy"
            :aria-expanded="showAccounts"
            @click="showAccounts = !showAccounts"
          >
            <span class="tc-user">{{ session.name || session.email }}</span>
            <span class="tc-user-caret">{{ showAccounts ? "▴" : "▾" }}</span>
          </button>
          <div v-if="session.email && session.name" class="tc-user-email">
            {{ session.email }}
          </div>
        </div>
        <button
          class="tc-btn tc-btn-ghost"
          type="button"
          :disabled="accountBusy"
          @click="onLogout"
        >
          Salir
        </button>
      </div>

      <div v-if="showAccounts" class="tc-accounts-panel">
        <button
          v-for="acc in accounts"
          :key="acc.email"
          type="button"
          class="tc-account-row"
          :class="{ active: acc.active }"
          :disabled="accountBusy"
          @click="onSwitchAccount(acc.email)"
        >
          <div class="tc-account-row-main">
            <span class="tc-account-name">{{ acc.name || acc.email }}</span>
            <span class="tc-account-email">{{ acc.email }}</span>
          </div>
          <span
            v-if="acc.active"
            class="tc-account-badge"
          >Activa</span>
          <button
            v-else
            class="tc-account-remove"
            type="button"
            title="Quitar cuenta"
            :disabled="accountBusy"
            @click.stop="onRemoveAccount(acc.email)"
          >
            ×
          </button>
        </button>
        <label class="tc-account-browser-label" for="account-browser-select">
          Navegador para agregar cuenta
        </label>
        <select
          id="account-browser-select"
          v-model="browserId"
          class="tc-account-browser"
          :disabled="loginWaiting"
          @change="onBrowserChange"
        >
          <option
            v-for="b in browsers"
            :key="b.id"
            :value="b.id"
            :disabled="!b.available"
          >
            {{ b.name }}{{ b.available ? "" : " (no instalado)" }}
          </option>
        </select>
        <p class="tc-account-hint">
          Elige el navegador donde ya tienes esa cuenta de Google iniciada.
        </p>
        <button
          v-if="!loginWaiting"
          class="tc-account-add"
          type="button"
          :disabled="accountBusy"
          @click="onAddAccount"
        >
          + Agregar cuenta
        </button>
        <div v-else class="tc-account-waiting">
          <span>Esperando Google…</span>
          <button class="tc-btn tc-btn-light" type="button" @click="onCancelLogin">
            Cancelar
          </button>
        </div>
      </div>

      <div class="tc-sidebar-actions">
        <button class="tc-btn tc-btn-primary tc-btn-block" type="button" @click="showNewChat = true">
          Nuevo chat
        </button>
      </div>

      <div class="tc-search">
        <input v-model="filter" type="search" placeholder="Buscar conversación…" />
      </div>

      <div class="tc-conv-list">
        <div v-if="loadingSpaces && !spaces.length" class="px-3 py-2 small opacity-75">
          Cargando conversaciones…
        </div>
        <button
          v-for="space in filteredSpaces"
          :key="space.name"
          type="button"
          class="tc-conv"
          :class="{ active: selected?.name === space.name }"
          @click="selectSpace(space)"
        >
          <span class="tc-conv-title">{{ space.display_name }}</span>
          <span class="tc-conv-meta">{{ spaceTypeLabel(space.space_type) }}</span>
        </button>
        <div v-if="!loadingSpaces && !filteredSpaces.length" class="px-3 py-2 small opacity-75">
          No hay conversaciones.
        </div>
      </div>
    </aside>

    <section class="tc-main">
      <template v-if="selected">
        <header class="tc-main-header">
          <div>
            <h3>{{ selected.display_name }}</h3>
            <div class="tc-status">{{ spaceTypeLabel(selected.space_type) }}</div>
          </div>
          <button class="tc-btn tc-btn-light" type="button" @click="refreshMessages(false)">
            Actualizar
          </button>
        </header>

        <div ref="messagesEl" class="tc-messages">
          <div v-if="loadingMessages" class="tc-empty">Cargando mensajes…</div>
          <div
            v-for="msg in messages"
            :key="msg.name"
            class="tc-bubble-row"
            :class="{ mine: msg.mine }"
          >
            <div class="tc-bubble">
              <div v-if="!msg.mine" class="tc-bubble-author">{{ msg.sender_name }}</div>
              <MessageAttachment
                v-if="msg.attachment_resource || msg.attachment_name"
                :resource-name="msg.attachment_resource"
                :mime="msg.attachment_mime"
                :file-name="msg.attachment_name"
              />
              <div v-if="msg.text" class="tc-bubble-text">{{ msg.text }}</div>
              <div
                v-if="!msg.text && !msg.attachment_resource && !msg.attachment_name"
                class="tc-bubble-text"
              >
                (sin texto / tarjeta)
              </div>
              <div class="tc-bubble-time">{{ formatTime(msg.create_time) }}</div>
            </div>
          </div>
        </div>

        <div v-if="pendingFile" class="tc-pending-image">
          <img
            v-if="pendingFile.isImage && pendingFile.previewUrl"
            :src="pendingFile.previewUrl"
            alt="Vista previa"
          />
          <div v-else class="tc-pending-file-icon">📎</div>
          <div class="tc-pending-meta">
            <span>{{ pendingFile.filename }}</span>
            <button class="tc-btn tc-btn-light" type="button" @click="clearPendingFile">
              Quitar
            </button>
          </div>
        </div>

        <form class="tc-composer" @submit.prevent="onSend">
          <textarea
            v-model="draft"
            rows="2"
            placeholder="Escribe… Ctrl+V pega imagen · Adjuntar archivo con el botón"
            @keydown="onKeydown"
            @paste="onPaste"
          />
          <div class="tc-composer-actions">
            <input
              ref="fileInput"
              type="file"
              class="tc-file-input"
              @change="onFilePicked"
            />
            <button
              class="tc-btn tc-btn-light"
              type="button"
              :disabled="sending"
              @click="openFilePicker"
            >
              Adjuntar
            </button>
            <button
              class="tc-btn tc-btn-light"
              type="button"
              :disabled="sending"
              title="Pegar imagen del portapapeles"
              @click="pasteFromClipboardButton"
            >
              Pegar imagen
            </button>
            <button class="tc-btn tc-btn-primary" type="submit" :disabled="!canSend">
              {{ sending ? "…" : pendingFile ? "Enviar archivo" : "Enviar" }}
            </button>
          </div>
        </form>
      </template>

      <div v-else class="tc-empty">
        <h3>Elige una conversación</h3>
        <p>Elige un chat a la izquierda o crea uno nuevo. La ventana puede quedar en la bandeja.</p>
      </div>

      <div v-if="error" class="px-3 pb-3">
        <div class="tc-alert error">{{ error }}</div>
      </div>
    </section>

    <NewChatModal
      v-if="showNewChat"
      @close="showNewChat = false"
      @started="onChatStarted"
    />
  </div>
</template>
