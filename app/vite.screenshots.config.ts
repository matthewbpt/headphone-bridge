import { defineConfig, mergeConfig } from "vite";
import viteConfig from "./vite.config.ts";
import vitestConfig from "./vitest.config.ts";

// Serves screenshots.html: the window against the fake core, with the Tauri
// APIs swapped out as the tests swap them.
export default mergeConfig(
  viteConfig,
  defineConfig({
    resolve: vitestConfig.resolve,
    server: { port: 1421 },
  }),
);
