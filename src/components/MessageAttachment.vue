<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { downloadAttachment } from "../services/tauri";

const props = defineProps<{
  resourceName?: string | null;
  mime?: string | null;
  fileName?: string | null;
}>();

const dataUrl = ref<string | null>(null);
const loading = ref(false);
const err = ref("");

const isImage = () => (props.mime || "").startsWith("image/");

async function load() {
  dataUrl.value = null;
  err.value = "";
  if (!props.resourceName) return;
  loading.value = true;
  try {
    dataUrl.value = await downloadAttachment(
      props.resourceName,
      props.mime || undefined,
    );
  } catch (e) {
    err.value = String(e);
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  void load();
});

watch(
  () => props.resourceName,
  () => {
    void load();
  },
);
</script>

<template>
  <div class="tc-attach">
    <div v-if="loading" class="tc-status">Cargando adjunto…</div>
    <div v-else-if="err" class="tc-attach-fallback">
      📎 {{ fileName || "archivo" }}
      <div class="tc-status">{{ err }}</div>
    </div>
    <template v-else-if="dataUrl">
      <img
        v-if="isImage()"
        class="tc-bubble-image"
        :src="dataUrl"
        :alt="fileName || 'imagen'"
      />
      <a
        v-else
        class="tc-attach-file"
        :href="dataUrl"
        :download="fileName || 'archivo'"
      >
        📎 {{ fileName || "Descargar archivo" }}
      </a>
    </template>
    <div v-else-if="fileName" class="tc-attach-fallback">📎 {{ fileName }}</div>
  </div>
</template>
