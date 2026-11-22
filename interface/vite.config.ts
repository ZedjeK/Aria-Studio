/// <reference types="vitest/config" />
// Interface d'Aria Studio : Vite compile le Svelte dans dist\, que Tauri embarque dans l'exe.
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vite';

export default defineConfig({
  plugins: [svelte()],
  // Le port de développement que Tauri attend (devUrl de appli\tauri.conf.json).
  server: { port: 1420, strictPort: true, host: '127.0.0.1' },
  clearScreen: false,
  build: {
    // WebView2 suit Chromium : pas besoin de code pour les vieux navigateurs.
    target: 'chrome120',
    // Pas de police ni d'image glissée dans le code en data: : la politique de sécurité ne l'accepte pas.
    assetsInlineLimit: 0,
    sourcemap: false,
  },
  test: {
    include: ['src/**/*.test.ts'],
    environment: 'node',
  },
});
