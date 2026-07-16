import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';

export default defineConfig({
  base: './',
  plugins: [vue()],
  optimizeDeps: {
    entries: ['index.html'],
  },
  build: {
    outDir: 'dist/renderer',
    emptyOutDir: true,
  },
});
