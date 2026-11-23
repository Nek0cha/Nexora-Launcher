import tailwindcss from "@tailwindcss/vite";
import { defineConfig } from "astro/config";
import Icons from "unplugin-icons/vite";

export default defineConfig({
  site: "https://nexora.ny4n.net",
  trailingSlash: "always",
  vite: {
    plugins: [tailwindcss(), Icons({ compiler: "astro" })],
  },
});
