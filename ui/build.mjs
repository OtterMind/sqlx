import { build } from "esbuild";
await build({
  entryPoints: ["src/app.ts"],
  bundle: true,
  format: "esm",
  target: "es2022",
  outfile: "dist/app.js",
  minify: true,
});
