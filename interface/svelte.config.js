// Svelte lit le TypeScript des composants grâce à Vite.
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte';

export default {
  preprocess: vitePreprocess(),
};
