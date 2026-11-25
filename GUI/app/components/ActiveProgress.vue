<template>
  <div class="mt-4 active-progress-zone pulse-glow">
    <div class="d-flex align-center justify-start mb-3">
      <v-icon color="blue-darken-2" class="mr-2 spinning-icon" size="22">mdi-autorenew</v-icon>
      <div class="text-subtitle-1 font-weight-bold text-blue-darken-4 text-truncate">
        {{ cleanPrefix }}
      </div>
    </div>

    <v-progress-linear
        :model-value="progressPercent"
        height="26"
        color="blue-darken-2"
        rounded
        striped
        class="active-bar"
    >
      <template v-slot:default="{ value }">
        <span class="text-white font-weight-black text-caption px-2 drop-shadow">
          {{ value.toFixed(2) }}%
        </span>
      </template>
    </v-progress-linear>

    <div class="d-flex justify-space-between mt-2">
      <div class="text-caption text-blue-grey-darken-2 font-weight-bold text-left">
        {{ counts }} &nbsp;|&nbsp; Elapsed: {{ elapsed }} &nbsp;|&nbsp; ETA: {{ eta }}
      </div>
      <div class="text-caption text-blue-grey-darken-2 font-weight-bold text-right">
        {{ rate }}
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed } from 'vue'

// `counts` et `rate` : textes déjà mis en forme (utils/progress_format.js), en MB / GB pour la lecture et
// l'écriture des fichiers, en nombre d'éléments pour les autres étapes
const props = defineProps({
  prefix: { type: String, required: true },
  progressPercent: { type: Number, required: true },
  counts: { type: String, required: true },
  elapsed: { type: String, required: true },
  eta: { type: String, required: true },
  rate: { type: String, required: true }
})

// Supprime automatiquement les deux-points ":" et les espaces superflus à la fin du texte Python
const cleanPrefix = computed(() => {
  return props.prefix.replace(/:\s*$/, '')
})
</script>

<style scoped>
.active-progress-zone {
  border: 2px solid #2196F3;
  background: linear-gradient(145deg, rgba(33, 150, 243, 0.05) 0%, rgba(33, 150, 243, 0.1) 100%);
  padding: 16px;
  border-radius: 12px;
  position: relative;
  overflow: hidden;
}

.pulse-glow {
  animation: pulse-border 2s infinite;
}

@keyframes pulse-border {
  0% { box-shadow: 0 0 0 0 rgba(33, 150, 243, 0.4); }
  70% { box-shadow: 0 0 0 8px rgba(33, 150, 243, 0); }
  100% { box-shadow: 0 0 0 0 rgba(33, 150, 243, 0); }
}

.spinning-icon {
  animation: spin 1.5s linear infinite;
}

@keyframes spin {
  100% { transform: rotate(360deg); }
}

:deep(.v-progress-linear__determinate) {
  transition: none !important;
}

.active-bar {
  border: 1px solid #1565C0;
  box-shadow: inset 0 2px 4px rgba(0,0,0,0.15);
}

.drop-shadow {
  text-shadow: 1px 1px 2px rgba(0,0,0,0.6);
  letter-spacing: 1px;
}
</style>