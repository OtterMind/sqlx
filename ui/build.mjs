import { build } from "esbuild";
import { createHash } from "node:crypto";
import { copyFile, mkdir, readFile, rm, writeFile } from "node:fs/promises";
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
  if (source === ".") {
    await copyFile("node_modules/gridstack/dist/gridstack.min.css", `${output}/gridstack.css`);
    await build({
      entryPoints: ["src/theme-bootstrap.ts"],
      bundle: true,
      format: "iife",
      target: "es2022",
      outfile: `${output}/theme.js`,
      minify: true,
    });
  }
  await Promise.all([
    copyFile(`${source}/style.css`, `${output}/app.css`),
    copyFile(`${source}/ui-plugin.json`, `${output}/ui-plugin.json`),
    copyFile("../LICENSE", `${output}/LICENSE`),
    copyFile("../NOTICE", `${output}/NOTICE`),
  ]);
  if (source === ".") {
    // The plugin keeps one version while it is being iterated on, so the scripts and styles carry a
    // content stamp: a rebuilt bundle gets a new URL and an open tab can never keep serving the old one.
    const stamp = async file => (await createHash("sha256")
      .update(await readFile(`${output}/${file}`))
      .digest("hex")).slice(0, 10);
    const [script, styles, theme] = await Promise.all([stamp("app.js"), stamp("app.css"), stamp("theme.js")]);
    const html = (await readFile(`${source}/index.html`, "utf8"))
      .replace('src="app.js"', `src="app.js?v=${script}"`)
      .replace('href="app.css"', `href="app.css?v=${styles}"`)
      .replace('src="theme.js"', `src="theme.js?v=${theme}"`);
    await writeFile(`${output}/index.html`, html);
  } else {
    await copyFile(`${source}/index.html`, `${output}/index.html`);
  }
}
