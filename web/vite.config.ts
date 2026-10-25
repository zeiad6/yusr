import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// yusr-server serves the built `dist/` itself (YUSR_UI_DIR); in dev, Vite proxies /api to it.
const API = process.env.YUSR_API_URL || "http://127.0.0.1:8080";

export default defineConfig({
  plugins: [react()],
  build: {
    rollupOptions: {
      output: {
        manualChunks: {
          react: ["react", "react-dom", "react-router-dom"],
          charts: ["recharts"],
          icons: ["lucide-react"],
        },
      },
    },
  },
  server: {
    host: true,
    port: 5173,
    proxy: { "/api": { target: API, changeOrigin: true } },
  },
});
