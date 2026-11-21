import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import vueJsx from "@vitejs/plugin-vue-jsx";

export default function boardViteConfig() {
  return defineConfig({
    plugins: [vue(), vueJsx()],
    define: { __VUE_OPTIONS_API__: false },
  });
}
