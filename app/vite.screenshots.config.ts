import { defineConfig, mergeConfig } from "vite";
import viteConfig from "./vite.config.ts";
import vitestConfig from "./vitest.config.ts";

// Serves screenshots.html: the window against the fake core, with the Tauri
// APIs swapped out as the tests swap them.
export default defineConfig((env) =>
  mergeConfig(
    viteConfig(env),
    defineConfig({
      resolve: vitestConfig(env).resolve,
      server: { port: 1421 },
    }),
  ),
);
