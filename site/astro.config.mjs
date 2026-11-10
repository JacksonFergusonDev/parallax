import { defineConfig } from 'astro/config';

export default defineConfig({
  site: 'https://parallax.jacksonferguson.me',
  output: 'static',
  server: ({ command }) => ({
    host: '127.0.0.1',
    port: command === 'dev' ? 4600 : 4602,
  }),
  trailingSlash: 'always',
  devToolbar: { enabled: false },
  vite: {
    server: { strictPort: true },
    preview: { strictPort: true },
  },
});
