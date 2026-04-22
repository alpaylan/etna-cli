import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { resolve } from 'path';

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: 'dist',
    rollupOptions: {
      // Two HTML entries share the same React bundle. `index.html` is loaded
      // by the VS Code extension; `site.html` is the standalone static
      // catalog, published by `etna workload site` and deployable anywhere.
      input: {
        index: resolve(__dirname, 'index.html'),
        site: resolve(__dirname, 'site.html'),
      },
      output: {
        entryFileNames: 'assets/[name].js',
        chunkFileNames: 'assets/[name].js',
        assetFileNames: 'assets/[name].[ext]',
      },
    },
  },
  base: './',
});
