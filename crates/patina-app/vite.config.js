import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath } from 'node:url';

export default defineConfig({
  plugins: [svelte()],
  root: 'web',
  clearScreen: false,
  resolve: {
    alias: {
      h5wasm: fileURLToPath(new URL('./web/vendor/h5wasm-stub.js', import.meta.url)),
      'happy-dom': fileURLToPath(new URL('./web/vendor/happy-dom-stub.js', import.meta.url))
    }
  },
  server: {
    port: 1420,
    strictPort: true
  },
  preview: {
    port: 1420,
    strictPort: true
  },
  build: {
    outDir: '../dist',
    emptyOutDir: true,
    chunkSizeWarningLimit: 850,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes('node_modules')) {
            return;
          }

          if (id.includes('/svelteplot/') || id.includes('/d3-') || id.includes('/d3/')) {
            return 'viz-plots';
          }

          if (id.includes('/three/')) {
            return 'three-core';
          }

          if (
            id.includes('@threlte/') ||
            id.includes('camera-controls') ||
            id.includes('troika-three') ||
            id.includes('three-instanced-uniforms-mesh') ||
            id.includes('three-mesh-bvh')
          ) {
            return 'viz-3d';
          }

          if (id.includes('/matterviz/') || id.includes('/svelte-multiselect/')) {
            return 'matterviz';
          }

          if (id.includes('@spglib/moyo-wasm')) {
            return 'moyo';
          }

          if (id.includes('@tauri-apps')) {
            return 'tauri';
          }

          if (id.includes('/svelte/')) {
            return 'svelte-vendor';
          }
        }
      }
    }
  }
});
