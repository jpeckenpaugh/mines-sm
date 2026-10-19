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

const path = require("node:path");

const enginePath = path.join(__dirname, "..", "engine.js");
const { runConformance } = require(enginePath);

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
