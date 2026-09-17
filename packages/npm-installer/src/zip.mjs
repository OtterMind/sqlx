import fs from "node:fs";
import path from "node:path";
import zlib from "node:zlib";

import { InstallError } from "./errors.mjs";

const END_OF_CENTRAL_DIRECTORY = 0x06054b50;
const CENTRAL_FILE_HEADER = 0x02014b50;
const LOCAL_FILE_HEADER = 0x04034b50;
const ZIP64_MARKER = 0xffffffff;
const MAXIMUM_COMMENT = 0xffff;
const UNIX_FILE_TYPE = 0xf000;
const UNIX_SYMBOLIC_LINK = 0xa000;
const UNIX_DIRECTORY = 0x4000;

function centralDirectoryEnd(buffer) {
  const earliest = Math.max(0, buffer.length - MAXIMUM_COMMENT - 22);
  for (let offset = buffer.length - 22; offset >= earliest; offset -= 1) {
    if (buffer.readUInt32LE(offset) === END_OF_CENTRAL_DIRECTORY) {
      return offset;
    }
  }
  throw new InstallError("archive", "downloaded archive is not a ZIP file");
}

function safeName(raw) {
  if (raw.includes("\\") || raw.includes(":")) {
    return null;
  }
  const trimmed = raw.replace(/\/+$/, "");
  if (trimmed === "") {
    return null;
  }
  const parts = trimmed.split("/");
  if (parts.some((part) => part === "" || part === "." || part === "..")) {
    return null;
  }
  return trimmed;
}

export function extractZip(buffer, destination) {
  const end = centralDirectoryEnd(buffer);
  const entries = buffer.readUInt16LE(end + 10);
  const centralSize = buffer.readUInt32LE(end + 12);
  const centralOffset = buffer.readUInt32LE(end + 16);
  if (centralOffset === ZIP64_MARKER || centralSize === ZIP64_MARKER || entries === 0xffff) {
    throw new InstallError("archive", "ZIP64 archives are not supported");
  }
  if (centralOffset + centralSize > buffer.length) {
    throw new InstallError("archive", "ZIP central directory is out of bounds");
  }
  const written = [];
  let offset = centralOffset;
  for (let index = 0; index < entries; index += 1) {
    if (offset + 46 > buffer.length || buffer.readUInt32LE(offset) !== CENTRAL_FILE_HEADER) {
      throw new InstallError("archive", "ZIP central directory is damaged");
    }
    const flags = buffer.readUInt16LE(offset + 8);
    const method = buffer.readUInt16LE(offset + 10);
    const compressedSize = buffer.readUInt32LE(offset + 20);
    const uncompressedSize = buffer.readUInt32LE(offset + 24);
    const nameLength = buffer.readUInt16LE(offset + 28);
    const extraLength = buffer.readUInt16LE(offset + 30);
    const commentLength = buffer.readUInt16LE(offset + 32);
    const externalAttributes = buffer.readUInt32LE(offset + 38);
    const localOffset = buffer.readUInt32LE(offset + 42);
    if (offset + 46 + nameLength > buffer.length) {
      throw new InstallError("archive", "ZIP central directory is damaged");
    }
    const rawName = buffer.toString("utf8", offset + 46, offset + 46 + nameLength);
    offset += 46 + nameLength + extraLength + commentLength;

    if (flags & 0x1) {
      throw new InstallError("archive", "encrypted ZIP entries are not supported");
    }
    if (
      compressedSize === ZIP64_MARKER ||
      uncompressedSize === ZIP64_MARKER ||
      localOffset === ZIP64_MARKER
    ) {
      throw new InstallError("archive", "ZIP64 entries are not supported");
    }
    if (((externalAttributes >>> 16) & UNIX_FILE_TYPE) === UNIX_SYMBOLIC_LINK) {
      throw new InstallError("archive", `ZIP symbolic links are not supported: ${rawName}`);
    }
    const name = safeName(rawName);
    if (name === null) {
      throw new InstallError("archive", `unsafe archive path: ${rawName}`);
    }
    if (method !== 0 && method !== 8) {
      throw new InstallError("archive", `unsupported ZIP compression method ${method} for ${name}`);
    }
    if (localOffset + 30 > buffer.length || buffer.readUInt32LE(localOffset) !== LOCAL_FILE_HEADER) {
      throw new InstallError("archive", `ZIP local header is damaged for ${name}`);
    }
    const start =
      localOffset + 30 + buffer.readUInt16LE(localOffset + 26) + buffer.readUInt16LE(localOffset + 28);
    if (start + compressedSize > buffer.length) {
      throw new InstallError("archive", `ZIP entry data is out of bounds for ${name}`);
    }
    const target = path.join(destination, name);
    if (((externalAttributes >>> 16) & UNIX_FILE_TYPE) === UNIX_DIRECTORY || rawName.endsWith("/")) {
      fs.mkdirSync(target, { recursive: true });
      continue;
    }
    const raw = buffer.subarray(start, start + compressedSize);
    let content;
    try {
      content = method === 0 ? raw : zlib.inflateRawSync(raw);
    } catch (error) {
      throw new InstallError("archive", `could not decompress ${name}: ${error.message}`);
    }
    if (content.length !== uncompressedSize) {
      throw new InstallError("archive", `ZIP entry ${name} has an unexpected size`);
    }
    fs.mkdirSync(path.dirname(target), { recursive: true });
    const mode = (externalAttributes >>> 16) & 0o777;
    fs.writeFileSync(target, content, { mode: mode || 0o644, flag: "wx" });
    written.push(name);
  }
  return written;
}
