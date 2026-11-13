<template>
  <v-container fluid class="h-100 d-flex flex-column align-center justify-center pa-4">

    <v-row align="center" justify="center" class="mb-6">
      <v-col cols="auto" class="d-flex align-center">
        <v-switch
            v-model="parameters.calculate_de_novo"
            :true-value="1.0"
            :false-value="0.0"
            color="success"
            inset
            hide-details
            class="mr-4"
        ></v-switch>
        <span class="text-h6 font-weight-bold">Calculate fragment formula</span>
        <span class="text-caption text-error font-italic ml-4">
          (warning: not compatible with most reprocessing software)
        </span>
      </v-col>
    </v-row>

    <v-row align="center" justify="center">
      <v-col cols="auto" class="d-flex align-center">
        <span class="text-body-1 mr-4">ppm tolerance:</span>
        <v-text-field
            :model-value="texts.de_novo_ppm_tolerance"
            @update:model-value="update('de_novo_ppm_tolerance', $event)"
            :error-messages="errorOf('de_novo_ppm_tolerance')"
            type="text"
            inputmode="decimal"
            variant="outlined"
            density="compact"
            hide-details="auto"
            bg-color="white"
            style="width: 120px;"
        ></v-text-field>
      </v-col>
    </v-row>

    <div class="position-absolute" style="bottom: 10px; right: 10px;">
      <v-tooltip location="top" max-width="400">
        <template v-slot:activator="{ props }">
          <v-btn icon="mdi-information" variant="text" size="small" color="grey" v-bind="props"></v-btn>
        </template>
        <span class="text-body-2">
          Enables the de novo chemical formula calculation for each spectrum.<br>
          The PPM tolerance is used for the precision of the formula matching.<br>
          Warning, this option makes the output databases incompatible with most reprocessing software.
        </span>
      </v-tooltip>
    </div>

  </v-container>
</template>

<script setup>
import { useState } from '#imports'
import { useNumberFields } from '~/composables/useNumberFields'

const parameters = useState('parameters')

// Initialisation des valeurs par défaut si elles n'existent pas
const initDefaults = () => {
  if (parameters.value.calculate_de_novo === undefined) {
    parameters.value.calculate_de_novo = 0.0
  }
  if (parameters.value.de_novo_ppm_tolerance === undefined) {
    parameters.value.de_novo_ppm_tolerance = 10.0
  }
}

initDefaults()

// Tolérance : virgule décimale acceptée ; saisie invalide signalée, jamais envoyée au serveur (avant : 0 ppm)
const { texts, update, errorOf } = useNumberFields(['de_novo_ppm_tolerance'])
</script>