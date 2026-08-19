import { defineConfig } from 'vite';

export default defineConfig({
  // Only add this line if your index.html is located in src/
  root: 'src',
  build: {
    outDir: '../dist',
  },
});