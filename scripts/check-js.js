#!/usr/bin/env node
//
// Run the browser engine's conformance checks under Node.
//
// The deployed harness (`engine.js`) is a second implementation of the Rust
// state machine. This runner keeps it honest in CI: any drift between the two
// implementations fails the job.
//
// Usage: node scripts/check-js.js

"use strict";

// The root package.json sets "type": "module", so this file and `engine.js`
// are both loaded as ES modules. `engine.js` is written as a classic script
// (it must run unmodified in a browser <script> tag) and exposes its API on
// `globalThis.Minesweeper` when `module.exports` is unavailable. Import it for
// its side effect, then read the API off the global.
import "../engine.js";

const { runConformance } = globalThis.Minesweeper;

const checks = runConformance();
let failed = 0;

for (const check of checks) {
  if (check.pass) {
    console.log(`ok   ${check.name}`);
  } else {
    failed += 1;
    console.error(`FAIL ${check.name}${check.detail ? ` — ${check.detail}` : ""}`);
  }
}

const passed = checks.length - failed;
if (failed > 0) {
  console.error(`\n${passed}/${checks.length} conformance checks passed, ${failed} failed`);
  process.exit(1);
}

console.log(`\n${passed}/${checks.length} conformance checks passed`);
