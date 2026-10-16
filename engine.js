// Browser/Node mirror of the `mine-sm` Rust state machine.
//
// This is a faithful JavaScript port of `lib.rs` (SplitMix64 layout, safe-first
// click relocation, BFS cascade, strikes, win detection, and the observation /
// action-mask encodings). It powers the visual harness in index.html and is
// validated against golden values produced by the Rust engine (see GOLDENS and
// `runConformance`).
//
// Both `window.Minesweeper` and `module.exports` are populated so the same file
// works in the browser and under `node`.

(function (global) {
  "use strict";

  const MASK64 = (1n << 64n) - 1n;

  // --- SplitMix64 -----------------------------------------------------------

  class SplitMix64 {
    constructor(seed) {
      this.state = BigInt.asUintN(64, BigInt(seed));
    }
    nextU64() {
      this.state = (this.state + 0x9e3779b97f4a7c15n) & MASK64;
      let z = this.state;
      z = ((z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n) & MASK64;
      z = ((z ^ (z >> 27n)) * 0x94d049bb133111ebn) & MASK64;
      z = (z ^ (z >> 31n)) & MASK64;
      return z;
    }
    nextBounded(bound) {
      return Number(this.nextU64() % BigInt(bound));
    }
  }

  // --- bitboard helpers -----------------------------------------------------

  const bit = (c) => 1n << BigInt(c);

  function popcount(x) {
    let count = 0;
    while (x > 0n) {
      count += Number(x & 1n);
      x >>= 1n;
    }
    return count;
  }

  const NEIGH = (() => {
    const out = [];
    for (let cell = 0; cell < 64; cell++) {
      const r = (cell / 8) | 0;
      const c = cell % 8;
      const list = [];
      for (let dr = -1; dr <= 1; dr++) {
        for (let dc = -1; dc <= 1; dc++) {
          if (dr === 0 && dc === 0) continue;
          const nr = r + dr;
          const nc = c + dc;
          if (nr >= 0 && nr < 8 && nc >= 0 && nc < 8) list.push(nr * 8 + nc);
        }
      }
      out.push(list);
    }
    return out;
  })();

  function computeAdj(mineMask) {
    const adj = new Array(64).fill(0);
    for (let cell = 0; cell < 64; cell++) {
      let n = 0;
      for (const nb of NEIGH[cell]) if ((mineMask & bit(nb)) !== 0n) n++;
      adj[cell] = n;
    }
    return adj;
  }

  // --- board construction ---------------------------------------------------

  function makeBoard(mineMask, numBombs, seed, maxStrikes) {
    return {
      seed,
      numBombs,
      maxStrikes,
      mineMask: mineMask & MASK64,
      adj: computeAdj(mineMask),
      revealed: 0n,
      flagged: 0n,
      detonated: 0n,
      moveCount: 0,
      strikes: 0,
      terminal: false,
      won: false,
    };
  }

  function fromSeed(seed, numBombs, maxStrikes) {
    const target = Math.min(numBombs, 63);
    const rng = new SplitMix64(seed);
    let mask = 0n;
    let placed = 0;
    while (placed < target) {
      const c = rng.nextBounded(64);
      const b = bit(c);
      if ((mask & b) === 0n) {
        mask |= b;
        placed++;
      }
    }
    return makeBoard(mask, target, BigInt.asUintN(64, BigInt(seed)), maxStrikes);
  }

  function fromMask(mineMask, maxStrikes) {
    const m = mineMask & MASK64;
    return makeBoard(m, popcount(m), 0n, maxStrikes);
  }

  // --- transitions ----------------------------------------------------------

  function result(validMove, newlyRevealed, hitBomb, terminal, won) {
    return { validMove, newlyRevealed, hitBomb, terminal, won };
  }

  function relocate(b, hitCell) {
    b.mineMask &= ~bit(hitCell);
    for (let cand = 0; cand < 64; cand++) {
      if (cand === hitCell) continue;
      if ((b.mineMask & bit(cand)) === 0n) {
        b.mineMask |= bit(cand);
        break;
      }
    }
    b.adj = computeAdj(b.mineMask);
  }

  function step(b, actionType, cell) {
    if (b.terminal || cell >= 64) {
      return result(false, 0, false, b.terminal, b.won);
    }
    const cb = bit(cell);

    if (actionType === "flag") {
      if ((b.revealed & cb) !== 0n) return result(false, 0, false, false, false);
      b.flagged ^= cb;
      b.moveCount++;
      return result(true, 0, false, false, false);
    }

    // reveal
    if (((b.revealed | b.flagged) & cb) !== 0n) {
      return result(false, 0, false, false, false);
    }
    if (b.moveCount === 0 && (b.mineMask & cb) !== 0n) relocate(b, cell);
    b.moveCount++;

    if ((b.mineMask & cb) !== 0n) {
      b.strikes++;
      b.detonated |= cb;
      b.revealed |= cb;
      if (b.strikes >= b.maxStrikes) {
        b.terminal = true;
        b.won = false;
      }
      return result(true, 1, true, b.terminal, false);
    }

    // safe reveal with BFS cascade
    b.revealed |= cb;
    let newly = 1;
    if (b.adj[cell] === 0) {
      const queue = [cell];
      let head = 0;
      while (head < queue.length) {
        const cur = queue[head++];
        for (const n of NEIGH[cur]) {
          const nb = bit(n);
          if ((b.revealed & nb) === 0n && (b.flagged & nb) === 0n) {
            b.revealed |= nb;
            newly++;
            if (b.adj[n] === 0) queue.push(n);
          }
        }
      }
    }

    const totalSafe = 64 - b.numBombs;
    const safeRevealed = popcount(b.revealed & ~b.mineMask);
    if (safeRevealed >= totalSafe) {
      b.terminal = true;
      b.won = true;
    }
    return result(true, newly, false, b.terminal, b.won);
  }

  // --- agent-facing encodings ----------------------------------------------

  function observation(b) {
    const out = new Array(64);
    for (let i = 0; i < 64; i++) {
      const bb = bit(i);
      if ((b.detonated & bb) !== 0n) out[i] = -2.0;
      else if ((b.flagged & bb) !== 0n) out[i] = -0.5;
      else if ((b.revealed & bb) === 0n) out[i] = -1.0;
      else out[i] = b.adj[i] / 10.0;
    }
    return out;
  }

  function actionMask(b) {
    const out = new Array(64);
    for (let i = 0; i < 64; i++) {
      out[i] = (b.revealed | b.flagged) & bit(i) ? 0.0 : 1.0;
    }
    return out;
  }

  // --- playback helpers (ground-truth, never guesses) ----------------------

  function playWinTrace(mineMask, maxStrikes) {
    const b = fromMask(mineMask, maxStrikes || 1);
    const trace = [];
    while (!b.terminal) {
      let cell = -1;
      for (let c = 0; c < 64; c++) {
        const cb = bit(c);
        if ((mineMask & cb) === 0n && (b.revealed & cb) === 0n && (b.flagged & cb) === 0n) {
          cell = c;
          break;
        }
      }
      if (cell < 0) break;
      trace.push(step(b, "reveal", cell).newlyRevealed);
    }
    return trace;
  }

  function safeNumberedCell(mineMask) {
    const adj = computeAdj(mineMask);
    for (let c = 0; c < 64; c++) {
      if ((mineMask & bit(c)) === 0n && adj[c] > 0) return c;
    }
    return -1;
  }

  function lowestBomb(mineMask) {
    for (let c = 0; c < 64; c++) if (mineMask & bit(c)) return c;
    return -1;
  }

  function maskOf(cells) {
    let m = 0n;
    for (const c of cells) m |= bit(c);
    return m;
  }

  // --- goldens (produced by the Rust engine) -------------------------------

  const GOLDENS = {
    // BoardState::new(seed, bombs, Rules::default()).mine_mask
    seeds: [
      { seed: 42, bombs: 10, mask: 0x8004401020340048n },
      { seed: 6, bombs: 1, mask: 0x0000000000000001n },
      { seed: 1, bombs: 5, mask: 0x0200008040000802n },
      { seed: 7, bombs: 3, mask: 0x0000000010800004n },
      { seed: 0, bombs: 0, mask: 0x0000000000000000n },
    ],
    // Adaptive winning-line traces (Rust `win_trace`).
    traces: [
      { cells: [0], trace: [1, 62] },
      { cells: [1], trace: [1, 1, 61] },
      { cells: [3], trace: [63] },
      { cells: [27], trace: [63] },
      { cells: [0, 9], trace: [1, 1, 59, 1] },
      { cells: [0, 1], trace: [1, 61] },
      { cells: [0, 63], trace: [1, 61] },
      { cells: [0, 1, 8, 9], trace: [1, 59] },
      { cells: [0, 9, 18, 27], trace: [1, 1, 57, 1] },
      { cells: [18, 19, 20, 26, 28], trace: [58, 1] },
      { cells: [18, 19, 20, 26, 28, 34, 35, 36], trace: [55, 1] },
    ],
  };

  // --- conformance ----------------------------------------------------------

  function sameArray(a, b) {
    return a.length === b.length && a.every((v, i) => v === b[i]);
  }

  function runConformance() {
    const checks = [];
    const add = (name, pass, detail) => checks.push({ name, pass, detail: detail || "" });

    for (const g of GOLDENS.seeds) {
      const mask = fromSeed(g.seed, g.bombs, 1).mineMask;
      add(
        `seed ${g.seed} / ${g.bombs} bombs -> layout`,
        mask === g.mask,
        `got 0x${mask.toString(16).padStart(16, "0")}`
      );
    }

    for (const g of GOLDENS.traces) {
      const mask = maskOf(g.cells);
      const trace = playWinTrace(mask, 1);
      add(
        `win trace [${g.cells.join(",")}]`,
        sameArray(trace, g.trace),
        `got [${trace.join(",")}]`
      );
    }

    // Safe first click: every single-bomb board, click the bomb first.
    let safeFirst = true;
    for (let c = 0; c < 64; c++) {
      const b = fromMask(bit(c), 1);
      const r = step(b, "reveal", c);
      if (r.hitBomb || b.strikes !== 0) safeFirst = false;
    }
    add("safe first click (all 64 single-bomb boards)", safeFirst);

    // Empty board wins on the first reveal.
    const empty = fromMask(0n, 1);
    const er = step(empty, "reveal", 0);
    add("bomb-free board wins on first reveal", er.won && empty.revealed === MASK64);

    // Observation encoding reaches 0.8 at adjacency 8.
    const ring8 = maskOf([18, 19, 20, 26, 28, 34, 35, 36]);
    const rb = fromMask(ring8, 1);
    step(rb, "reveal", 27);
    add("adjacency 8 encodes as 0.8", observation(rb)[27] === 0.8);

    // Shielded rules survive two strikes, terminate on the third.
    const shieldedMask = maskOf([0, 1, 2]);
    const sb = fromMask(shieldedMask, 3);
    const s1 = step(sb, "reveal", 3); // safe numbered open
    const s2 = step(sb, "reveal", 0);
    const s3 = step(sb, "reveal", 1);
    const s4 = step(sb, "reveal", 2);
    add(
      "shielded ladder: survive twice, lose on 3rd",
      !s1.terminal && !s2.terminal && !s3.terminal && s4.terminal && !s4.won && sb.strikes === 3
    );

    return checks;
  }

  const api = {
    SplitMix64,
    fromSeed,
    fromMask,
    step,
    observation,
    actionMask,
    playWinTrace,
    safeNumberedCell,
    lowestBomb,
    maskOf,
    bit,
    popcount,
    GOLDENS,
    runConformance,
  };

  global.Minesweeper = api;
  if (typeof module !== "undefined" && module.exports) module.exports = api;
})(typeof window !== "undefined" ? window : globalThis);
