<template>
  <v-container fluid class="h-100 position-relative d-flex flex-column align-center justify-center pa-4">

    <div class="d-flex flex-column align-center justify-center">

      <v-btn
          icon
          width="100"
          height="100"
          elevation="3"
          color="grey-lighten-4"
          class="mb-4"
          @click="browseFolder"
      >
        <v-icon size="50" color="primary">mdi-folder-open-outline</v-icon>
      </v-btn>

      <div class="text-subtitle-1 font-weight-bold mb-4">Select output directory</div>

      <div
          v-if="parameters.output_directory"
          class="text-body-1 text-center text-break px-4 text-primary font-weight-medium"
          style="max-width: 600px;"
      >
        {{ parameters.output_directory }}
      </div>

      <div v-else class="text-caption text-grey font-italic">
        No directory selected
      </div>

    </div>

    <div class="position-absolute" style="bottom: 10px; right: 10px;">
      <v-tooltip location="top" max-width="400">
        <template v-slot:activator="{ props }">
          <v-btn icon="mdi-information" variant="text" size="small" color="grey" v-bind="props"></v-btn>
        </template>
        <span class="text-body-2">
          Create a new empty directory or Select an existing directory <br>
          where FragHub has already written files
        </span>
      </v-tooltip>
    </div>

  </v-container>
</template>

<script setup>
// Dans la section <script setup> de OutputTab.vue :
import { useState } from '#imports'

const parameters = useState('parameters')

const browseFolder = async () => {
  if (window.electronAPI) {
    const selectedFolder = await window.electronAPI.selectFolder()

    if (selectedFolder) {
      // selectedFolder contiendra le chemin ABSOLU (ex: C:\Users\Axel\...)
      parameters.value.output_directory = selectedFolder
    }
  } else {
    console.warn("L'API Electron n'est pas disponible.")
  }
}
</script>