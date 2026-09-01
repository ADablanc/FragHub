<template>
  <v-container fluid class="h-100 d-flex flex-column align-center justify-center pa-4">

    <div class="d-flex flex-column">
      <v-switch
          v-for="item in formats" :key="item.key"
          v-model="parameters[item.key]"
          :true-value="1.0"
          :false-value="0.0"
          :label="item.label"
          color="success"
          inset
          hide-details
          class="scale-switch mb-2"
      ></v-switch>
    </div>

    <div class="position-absolute" style="bottom: 10px; right: 10px;">
      <v-tooltip location="top" max-width="400">
        <template v-slot:activator="{ props }">
          <v-btn icon="mdi-information" variant="text" size="small" color="grey" v-bind="props"></v-btn>
        </template>
        <span class="text-body-2">
          This tab lets you choose the output formats to be written by FragHub at the end of processing.
        </span>
      </v-tooltip>
    </div>

  </v-container>
</template>

<script setup>
import { useState } from '#imports'

const parameters = useState('parameters')

const formats = [
  { label: 'CSV', key: 'csv' },
  { label: 'MSP', key: 'msp' },
  { label: 'JSON', key: 'json' },
  { label: 'mzSpecLib (JSON)', key: 'mzspeclib_json' }
]

// Initialisation des valeurs par défaut à 1.0 (ON) comme dans le script Python
const initDefaults = () => {
  formats.forEach(f => {
    if (parameters.value[f.key] === undefined) {
      parameters.value[f.key] = 1.0
    }
  })
}

initDefaults()
</script>

<style scoped>
/* Pour garder la même taille de switch que les autres onglets */
.scale-switch {
  transform: scale(1.1);
}
</style>