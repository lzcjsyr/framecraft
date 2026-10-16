import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
export default defineConfig({
  plugins: [react()],
  server: { port: 1427, strictPort: true },
  clearScreen: false,
  build: { rollupOptions: { output: { manualChunks: { terminal: ["@xterm/xterm", "@xterm/addon-fit"] } } } },
});
