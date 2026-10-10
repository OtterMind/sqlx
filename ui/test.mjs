import { build } from "esbuild";
import assert from "node:assert/strict";
// Chart messages are user-visible and therefore translated: pin the locale so the assertions stay deterministic.
Object.defineProperty(globalThis, "navigator", { value: { languages: ["zh-CN"], language: "zh-CN" }, configurable: true });
globalThis.localStorage = { getItem: () => null, setItem: () => {}, removeItem: () => {} };
const compiled = await build({ entryPoints: ["src/chart-option.ts"], bundle: true, write: false, format: "esm", platform: "node" });
const { chartOption, validateSpec, tupleKey } = await import(`data:text/javascript;base64,${Buffer.from(compiled.outputFiles[0].text).toString("base64")}`);
const metric = (field, unit = "CNY", axis = "left", kind = null) => ({ field, label: "", unit, axis, kind });
const spec = (chart_type = "Column", group_by = []) => ({ chart_type, dimension: "month", group_by, metrics: [metric("revenue")], stack: false, line_style: "straight" });
const data = { columns: ["month", "region", "channel", "revenue", "rate"], rows: [["Jan", "North", "web", "10.25", "1.2"], ["Feb", "North", "web", "12.75", "1.3"], ["Jan", "South", "app", "20", "2.1"]] };
assert.notEqual(tupleKey([null]), tupleKey([""]));
assert.notEqual(tupleKey([1]), tupleKey(["1"]));
assert.notEqual(tupleKey(["a / b", "c"]), tupleKey(["a", "b / c"]));
assert.throws(() => validateSpec(spec(), data), /重复/);
const grouped = spec("Column", ["region", "channel"]); grouped.stack = true;
const option = chartOption(grouped, data);
assert.equal(option.series.length, 2);
assert.deepEqual(option.series[1].data, [20, null]);
assert.equal(option.series[0].stack, option.series[1].stack);
const combo = { ...grouped, chart_type: "Combo", metrics: [metric("revenue", "CNY", "left", "Column"), metric("rate", "%", "right", "Line")] };
const mixed = chartOption(combo, data);
assert.equal(mixed.series.length, 4); assert.equal(mixed.yAxis.length, 2);
assert.equal(mixed.series[1].yAxisIndex, 1); assert.equal(mixed.series[1].stack, undefined);
const units = { ...combo, metrics: [metric("revenue", "CNY", "left", "Column"), metric("rate", "%", "left", "Line")] };
assert.throws(() => validateSpec(units, data), /单位/);
assert.throws(() => validateSpec(grouped, { ...data, rows: Array.from({length:33}, (_, i) => ["Jan", i, "web", 10, 1]) }), /32/);
const sameX = { columns: ["month", "revenue"], rows: [[1, 2], [1, 4]] };
assert.doesNotThrow(() => validateSpec(spec("Scatter"), sameX));
for (const type of ["Column", "Bar", "Line", "AreaLine", "Scatter", "Pie", "RingPie", "RosePie", "Funnel", "WordCloud", "Combo"]) {
  const s = spec(type); if (type === "Combo") s.metrics[0].kind = "Column";
  const o = chartOption(s, { columns: ["month", "revenue"], rows: [[1, 4], [2, 8]] });
  assert.ok(o.series.length, type);
}
assert.throws(() => validateSpec(spec("Pie"), {columns:["month","revenue"], rows:[["a", -1]]}), /负值/);
assert.throws(() => validateSpec(spec("Column"), {columns:["month","revenue"], rows:[["a", "9007199254740993"]]}), /精度/);
assert.throws(() => validateSpec(spec("Scatter"), {columns:["month","revenue"],rows:[["text",1]]}), /数值/);
assert.doesNotThrow(() => validateSpec({...spec("Statistics"),dimension:null}, {columns:["revenue"],rows:[["9007199254740993"]]}));
assert.throws(() => validateSpec({...spec("Statistics"), dimension:null}, sameX), /一行/);
assert.doesNotThrow(() => validateSpec({...spec("Statistics"), dimension:null}, {columns:["revenue"],rows:[["123.4567890123456789"]]}));
assert.doesNotThrow(() => validateSpec({...spec("Table"),metrics:[],dimension:null}, sameX));
console.log("Chart contracts passed: 13 types, typed tuples, grain, gaps, series limits, units, axes and exact-data boundaries.");

// Catalogue guard: both locales stay in lockstep, and every key used in code exists.
const fs = await import("node:fs/promises");
const path = await import("node:path");
const readJson = async file => JSON.parse(await fs.readFile(file, "utf8"));
const english = await readJson("src/i18n/locales/en-US.json");
const chinese = await readJson("src/i18n/locales/zh-CN.json");
assert.deepEqual(Object.keys(chinese).sort(), Object.keys(english).sort(), "locale catalogues diverged");
const sources = [];
for (const dir of ["src", "sdk"])
  for (const entry of await fs.readdir(dir, { withFileTypes: true }))
    if (entry.isFile() && entry.name.endsWith(".ts")) sources.push(path.join(dir, entry.name));
const used = new Set();
for (const file of sources) {
  const text = await fs.readFile(file, "utf8");
  for (const match of text.matchAll(/\bt\(\s*"([^"]+)"/g)) used.add(match[1]);
  for (const match of text.matchAll(/\bkey\(\s*"([^"]+)"/g)) used.add(match[1]);
}
for (const match of (await fs.readFile("index.html", "utf8")).matchAll(/data-i18n="([^"]+)"/g)) used.add(match[1]);
assert.deepEqual([...used].filter(key => !(key in english)).sort(), [], "translation keys used in code but missing from the catalogue");
console.log(`i18n catalogue passed: ${Object.keys(english).length} keys in both locales, ${used.size} keys referenced from code.`);
