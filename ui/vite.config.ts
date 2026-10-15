import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import Icons from "unplugin-icons/vite";

export default defineConfig({
  plugins: [react(), tailwindcss(), Icons({ compiler: "jsx", jsx: "react" })],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { target: "es2022" },
  test: { environment: "jsdom" },
});
