import { createHash } from "node:crypto";
import fs from "node:fs";

import { InstallError } from "./errors.mjs";

const DEFAULT_RELEASE_BASE = "https://github.com/OtterMind/sqlx/releases";
const METADATA_TIMEOUT_MS = 30_000;
const ARCHIVE_TIMEOUT_MS = 600_000;
const LOOPBACK_HOSTS = new Set(["localhost", "127.0.0.1", "[::1]"]);

let metadata;

function packageMetadata() {
  metadata ??= JSON.parse(fs.readFileSync(new URL("../package.json", import.meta.url), "utf8"));
  return metadata;
}

export function targetVersion() {
  return process.env.SQLX_VERSION?.trim() || packageMetadata().version;
}

export function platformName() {
  const os = { darwin: "macos", linux: "linux", win32: "windows" }[process.platform];
  if (!os) {
    throw new InstallError("platform", `unsupported operating system: ${process.platform}`);
  }
  const arch = { arm64: "arm64", x64: "x64" }[process.arch];
  if (!arch) {
    throw new InstallError("platform", `unsupported CPU architecture: ${process.arch}`);
  }
  if (os === "windows" && arch !== "x64") {
    throw new InstallError("platform", "Windows ARM64 is not a release target");
  }
  return `${os}-${arch}`;
}

export function releaseBase() {
  return (process.env.SQLX_RELEASE_BASE?.trim() || DEFAULT_RELEASE_BASE).replace(/\/+$/, "");
}

export function compareVersions(left, right) {
  const parts = (value) => {
    const numbers = value.split(".").map((part) => (/^\d+$/.test(part) ? Number(part) : Number.NaN));
    if (numbers.some(Number.isNaN)) {
      throw new InstallError("version", `cannot compare versions ${left} and ${right}`);
    }
    return numbers;
  };
  const a = parts(left);
  const b = parts(right);
  for (let index = 0; index < Math.max(a.length, b.length); index += 1) {
    const difference = (a[index] ?? 0) - (b[index] ?? 0);
    if (difference !== 0) {
      return Math.sign(difference);
    }
  }
  return 0;
}

export function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function allowedUrl(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    return false;
  }
  if (url.username || url.password || url.search || url.hash) {
    return false;
  }
  if (url.protocol === "https:") {
    return true;
  }
  return url.protocol === "http:" && LOOPBACK_HOSTS.has(url.hostname);
}

function releaseUrl(base, version, file) {
  return `${base}/download/v${version}/${file}`;
}

async function fetchBytes(url, timeout, what) {
  let response;
  try {
    response = await fetch(url, {
      redirect: "follow",
      signal: AbortSignal.timeout(timeout),
      headers: { "user-agent": `${packageMetadata().name}/${packageMetadata().version}` },
    });
  } catch (error) {
    throw new InstallError("download", `could not download ${what} from ${url}: ${error.message}`);
  }
  if (!response.ok) {
    throw new InstallError("download", `could not download ${what} from ${url}: HTTP ${response.status}`);
  }
  return Buffer.from(await response.arrayBuffer());
}

function parseSums(text) {
  const sums = new Map();
  for (const line of text.split("\n")) {
    const match = /^([0-9a-fA-F]{64})\s+\*?(.+?)\s*$/.exec(line);
    if (match) {
      sums.set(match[2], match[1].toLowerCase());
    }
  }
  return sums;
}

function validEntrypoint(value) {
  return (
    typeof value === "string" &&
    value.length > 0 &&
    value !== "." &&
    value !== ".." &&
    !value.includes("/") &&
    !value.includes("\\")
  );
}

export async function loadRelease(version, platform) {
  const base = releaseBase();
  const sums = parseSums(
    (await fetchBytes(releaseUrl(base, version, "SHA256SUMS"), METADATA_TIMEOUT_MS, "SHA256SUMS")).toString("utf8"),
  );
  const manifestBytes = await fetchBytes(
    releaseUrl(base, version, "manifest.json"),
    METADATA_TIMEOUT_MS,
    "manifest.json",
  );
  if (sums.get("manifest.json") !== sha256(manifestBytes)) {
    throw new InstallError("verify", "SHA256SUMS does not match the downloaded manifest.json");
  }
  let manifest;
  try {
    manifest = JSON.parse(manifestBytes.toString("utf8"));
  } catch (error) {
    throw new InstallError("verify", `release manifest is not valid JSON: ${error.message}`);
  }
  if (manifest?.schema_version !== 1) {
    throw new InstallError("verify", "unsupported release manifest schema");
  }
  const asset = manifest.components?.[`cli:${platform}`];
  if (!asset) {
    throw new InstallError("verify", `release ${version} has no CLI build for ${platform}`);
  }
  if (asset.version !== version) {
    throw new InstallError("verify", `release manifest CLI version ${asset.version} does not match ${version}`);
  }
  if (asset.archive !== "zip") {
    throw new InstallError("verify", `unsupported CLI archive format: ${asset.archive}`);
  }
  if (!validEntrypoint(asset.entrypoint)) {
    throw new InstallError("verify", `unsafe CLI entrypoint: ${asset.entrypoint}`);
  }
  if (!/^[0-9a-fA-F]{64}$/.test(asset.sha256 ?? "")) {
    throw new InstallError("verify", "release manifest has an invalid CLI SHA-256");
  }
  if (!allowedUrl(asset.url)) {
    throw new InstallError("verify", `unsafe CLI download URL: ${asset.url}`);
  }
  const name = new URL(asset.url).pathname.split("/").pop();
  if (!name || sums.get(name) !== asset.sha256.toLowerCase()) {
    throw new InstallError("verify", `SHA256SUMS and manifest disagree about ${name || "the CLI archive"}`);
  }
  return { version, platform, asset: { ...asset, name } };
}

export async function downloadAsset(asset) {
  const bytes = await fetchBytes(asset.url, ARCHIVE_TIMEOUT_MS, asset.name);
  if (sha256(bytes) !== asset.sha256.toLowerCase()) {
    throw new InstallError("verify", `checksum verification failed for ${asset.name}`);
  }
  return bytes;
}
