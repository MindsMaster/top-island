import { fileURLToPath, URL } from 'node:url';
import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';

/** xtask dev 探到的空闲端口 */
const devPort = Number(process.env.TOP_ISLAND_DEV_PORT) || 1420;

export default defineConfig({
  base: './',
  plugins: [vue()],
  resolve: {
    alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) },
  },
  optimizeDeps: {
    entries: ['index.html', 'settings.html'],
  },
  server: {
    port: devPort,
    strictPort: true,
    /** 避开 cargo 产物 防 EBUSY */
    watch: { ignored: ['**/target/**', '**/src-tauri/**', '**/crates/**', '**/xtask/**'] },
  },
  build: {
    outDir: 'dist/renderer',
    emptyOutDir: true,
    rollupOptions: {
      input: {
        island: fileURLToPath(new URL('./index.html', import.meta.url)),
        settings: fileURLToPath(new URL('./settings.html', import.meta.url)),
      },
    },
  },
});
