#!/usr/bin/env node
import { run } from "../src/main.mjs";

process.exitCode = await run(process.argv.slice(2));
