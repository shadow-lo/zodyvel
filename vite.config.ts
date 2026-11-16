import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Ada ve ayarlar ayrı sayfalar: ayarlar penceresi yalnız açıldığında yüklenir.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: { host: '127.0.0.1', port: 1420, strictPort: true },
  build: {
    target: 'es2022',
    cssCodeSplit: true,
    modulePreload: { polyfill: false },
    rollupOptions: {
      input: { island: 'index.html', settings: 'settings.html', update: 'update.html' },
    },
  },
  test: { include: ['tests/**/*.test.ts'] },
});
