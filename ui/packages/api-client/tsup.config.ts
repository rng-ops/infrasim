import { defineConfig } from "tsup";

export default defineConfig({
  entry: ["src/index.ts"],
  format: ["cjs", "esm"],
  dts: false,
  minify: true,
  sourcemap: true,
  clean: true,
  outExtension({ format }) {
    return format === "cjs" ? { js: ".cjs" } : { js: ".mjs" };
  },
});
