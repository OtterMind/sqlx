import { build } from "esbuild";
import { copyFile, mkdir, rm } from "node:fs/promises";
for (const [source, entrypoint] of [
  [".", "src/app.ts"],
  ["../examples/terminal-ui", "app.ts"],
]) {
  const output = `${source}/dist`;
  await rm(output, { recursive: true, force: true });
  await mkdir(output, { recursive: true });
  await build({
    entryPoints: [`${source}/${entrypoint}`],
    bundle: true,
    format: "esm",
    target: "es2022",
    outfile: `${output}/app.js`,
    minify: true,
  });
  await Promise.all([
    copyFile(`${source}/index.html`, `${output}/index.html`),
    copyFile(`${source}/style.css`, `${output}/app.css`),
    copyFile(`${source}/ui-plugin.json`, `${output}/ui-plugin.json`),
    copyFile("../LICENSE", `${output}/LICENSE`),
    copyFile("../NOTICE", `${output}/NOTICE`),
  ]);
}
