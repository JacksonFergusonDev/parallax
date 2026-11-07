import { defineConfig } from 'vite'
import { resolve } from 'node:path'
const stub = process.env.NO_PDF_STUB ? [] : [{ find: /^\.\/pdf\.js$/, replacement: resolve(__dirname, 'pdf-stub.js') }]
export default defineConfig({
  base: './', resolve: { alias: stub },
  build: { outDir: '../dist', emptyOutDir: true, target: 'esnext', modulePreload: false, minify: true,
    rollupOptions: { input: ['reader.html', 'reader-nometa.html', 'main.html'].map(f => resolve(__dirname, f)) } },
})
