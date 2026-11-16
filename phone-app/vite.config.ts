import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { host: '127.0.0.1', port: 1421, strictPort: true },
  build: { target: 'es2022', modulePreload: { polyfill: false } },
  test: { include: ['tests/**/*.test.ts'] },
});
