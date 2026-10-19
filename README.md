# mine-sm

[![CI](https://github.com/jpeckenpaugh/mines-sm/actions/workflows/ci.yml/badge.svg)](https://github.com/jpeckenpaugh/mines-sm/actions/workflows/ci.yml)

A deterministic, `no_std` 8×8 **Minesweeper state machine** designed as an
environment for reinforcement learning and reinforcement-learning-adjacent
tooling (e.g. ONNX policy evaluation).

The crate exposes a tiny, allocation-free API: build a board from a seed, apply
`Reveal`/`Flag` actions, and read back an observation tensor and an action mask
shaped for a neural network.

**Live demo:** <https://mines-sm.livive.net/> — a browser harness
that plays the board, shows the observation / action-mask encodings, auto-plays
winning and losing lines, and runs conformance checks against the Rust engine.

- **Deterministic** — the same `(seed, actions)` always produces the same game,
  and the PRNG (SplitMix64) is byte-for-byte identical on Wasm and native
  x86/ARM.
- **`no_std`** — no heap, no OS; a flat 64-cell `u64` bitboard.
- **Agent-shaped** — `get_observation` and `get_action_mask` return `[f32; 64]`.
- **Configurable rules** — classic sudden death or multi-strike ("shielded").

## Installation

Add it as a path or git dependency in your `Cargo.toml`:

```toml
[dependencies]
mine-sm = { git = "https://github.com/jpeckenpaugh/mines-sm" }
```

The library imports as `mine_sm`.

## Quickstart

```rust
use mine_sm::{Action, ActionType, BoardState, Rules};

// Deterministic board: seed 42, 10 bombs, classic sudden-death rules.
let mut board = BoardState::new(42, 10, Rules::default());

// The very first reveal is guaranteed safe.
let result = board.step(Action {
    action_type: ActionType::Reveal,
    cell: 27,
});
assert!(result.valid_move);
assert!(!result.hit_bomb);

// Flag a cell; revealing it afterwards is rejected.
board.step(Action { action_type: ActionType::Flag, cell: 0 });
let rejected = board.step(Action { action_type: ActionType::Reveal, cell: 0 });
assert!(!rejected.valid_move);

// Read the agent-facing tensors.
let mut observation = [0f32; 64];
let mut action_mask = [0f32; 64];
board.get_observation(&mut observation);
board.get_action_mask(&mut action_mask);
```

A full, runnable demo (renders the board and plays greedily until the game
ends) lives in [`examples/quickstart.rs`](examples/quickstart.rs):

```sh
cargo run --example quickstart
```

## API

| Item | Description |
| --- | --- |
| `BoardState::new(seed: u64, num_bombs: u8, rules: Rules) -> BoardState` | Build a deterministically seeded 8×8 board. `num_bombs` is clamped to 63. |
| `BoardState::from_mine_mask(mine_mask: u64, rules: Rules) -> BoardState` | Build a board with bombs at exactly the set bits of `mine_mask` (no seed; `seed` is `0`). Useful for pinned fixtures, replays, and benchmarks. A full mask has no safe cells and can never be won. |
| `BoardState::step(&mut self, action: Action) -> StepResult` | Apply an action and return its outcome. |
| `BoardState::get_observation(&self, out: &mut [f32; 64])` | Encode the *visible* board for a model. |
| `BoardState::get_action_mask(&self, out: &mut [f32; 64])` | 1.0 for legal reveal targets, 0.0 otherwise. All zeros once the game is terminal. |
| `Action { action_type: ActionType, cell: u8 }` | A move on a cell in `0..64`. |
| `ActionType::{Reveal, Flag}` | The two move kinds. |
| `Rules { max_strikes: u8 }` | `1` = classic sudden death, `3` = shielded, etc. |
| `StepResult { valid_move, newly_revealed, hit_bomb, terminal, won }` | Per-move feedback. |

Cells are indexed row-major: `cell = row * 8 + col`, with `row, col` in `0..8`.

## Observation encoding

`get_observation` fills `out[cell]` with:

| Value | Meaning |
| --- | --- |
| `-1.0` | Hidden (unrevealed) cell |
| `-0.5` | Flagged cell |
| `-2.0` | Detonated bomb |
| `0.0 ..= 0.8` | Revealed safe cell's adjacent-bomb count, `count / 10.0` |

## Rules modeled

- **Safe first click** — if the very first *reveal* would hit a bomb, that bomb
  is relocated to the lowest-index free cell before the reveal resolves. Flags
  do not count as reveals, so flagging first does not disable the guarantee.
  The exception is a full-mine `from_mine_mask` fixture: it has no free cell to
  relocate into, so the first reveal detonates and the board is unwinnable.
- **Cascade** — revealing a zero-adjacency cell flood-fills (BFS) through
  connected zero cells, revealing their numbered borders. Flagged cells are
  never opened by a cascade.
- **Strikes** — each detonated bomb increments `strikes`; the game ends when
  `strikes >= rules.max_strikes`.
- **Win** — all non-mine cells revealed.
- **Invalid moves** — revealing an opened/flagged cell, flagging an opened cell,
  acting after terminal, or using a cell `>= 64` all return `valid_move = false`
  and do not advance the board.

## Determinism

Bomb placement uses a hand-rolled SplitMix64 PRNG plus rejection sampling over
the 64-cell bitboard. There are no floating-point or platform-dependent
operations in game logic, so a seed fully determines the board across targets
(including Wasm).

## Testing

```sh
cargo test
```

For a readable pass/fail summary (fmt, clippy, and tests in one go — useful
because `cargo clippy` is silent when there are no findings):

```sh
./scripts/check.sh              # fmt + clippy + tests (debug, fast)
./scripts/check.sh --release    # same, compiled in release
./scripts/check.sh --exhaustive # release + the #[ignore]d exhaustive suites
```

Coverage is driven by **deterministic sampling**, so the default run is fast.
The exhaustive suites (which enumerate billions of boards) are never run unless
you pass `--exhaustive`; each `#[ignore]`d test can also be run directly, e.g.
`cargo test --release --test eval_multi_bomb -- --ignored`.

The integration suite in [`tests/minesweeper.rs`](tests/minesweeper.rs) covers
determinism, bomb placement/clamping, adjacency recomputation, the safe-first-
click relocation, flag semantics, cascade behavior, strike/terminal handling
for both classic and shielded rules, the win condition, and the exact
observation/action-mask encodings.

### Eval coverage: the 64 single-bomb boards

[`tests/eval_scenarios.rs`](tests/eval_scenarios.rs) runs an exhaustive eval
grid over every single-bomb layout: for each bomb position `0..64`, one
**winning** line (reveal all 63 safe cells, classic rules) and one **losing**
line (detonate the bomb), i.e. **64 win + 64 lose = 128 deterministic
fixtures**. The position axis covers every geometry (corner, edge, interior);
the outcome axis covers both terminal states. A few representative cases also
assert exact per-step `newly_revealed` traces to lock down the cascade shape.

Note that single-bomb boards are not trivially uniform: a cascade does not
always open the whole board. When the bomb sits on an edge, cells can be walled
off behind its numbered neighbours and take extra reveals (e.g. bomb at cell 1
has trace `[1, 1, 61]`), which is exactly the partial-cascade behavior the grid
is meant to pin down.

### Eval coverage: the 2-bomb boards

[`tests/eval_two_bombs.rs`](tests/eval_two_bombs.rs) extends the grid one bomb
deep: all `C(64, 2) = 2016` two-bomb layouts, each with a winning and losing
line (**2016 + 2016 = 4032 fixtures**). Two bombs can split the board into
multiple zero-regions, producing the partial/multi-region cascades the
single-bomb set cannot (e.g. bombs at cells 0 and 9 win with trace
`[1, 1, 59, 1]`). The file also verifies, via Burnside over the square's 8
symmetries, that the 2016 layouts collapse to **278** canonical geometries.

### Eval coverage: shielded rules (sampled)

Shielded-rule termination (`max_strikes = 3`) needs at least **three** distinct
bombs, so it can't reuse the 1- and 2-bomb grids.
[`tests/eval_shielded.rs`](tests/eval_shielded.rs) takes a **1% stride sample**
of the minimal 3-bomb family (every 100th of `C(64,3) = 41664` → **417**
layouts), plus 30 seeded layouts each for `k = 4..=8` where bombs remain after
the game ends. For each it verifies the strike ladder — strike 1 and 2 survive,
strike 3 terminates in a loss — that the terminal state's detonated/revealed
masks are consistent, and that shielded boards can still be won.

Remaining out of scope: exhaustive `k >= 3` enumeration (`C(64,3) = 41664` and
climbing fast) — sampling is the practical route there.

### Eval coverage: 4-bomb boards (sampled + exhaustive opt-in)

`C(64, 4) = 635376` is too large for the default run, so
[`tests/eval_four_bombs.rs`](tests/eval_four_bombs.rs) takes a **1% stride
sample** (every 100th → ~6354 layouts, win + lose each) plus curated structured
cases that uniform sampling only hits by luck:

- a **2×2 bomb block** and a **diagonal line** (multi-region cascades),
- a **four-corners** and an **edge-row** layout,
- a **centre count-4** layout (a cell adjacent to all four bombs), which also
  asserts the observation encoding reaches `4 / 10 = 0.4`.

Coverage assertions check the sample is meaningful: size ≈ 1%, the full
adjacency range up to 4 appears, both single-cascade and multi-step wins occur,
and bombs land in every cell class (corner / edge / interior).

The full enumeration is available as an `#[ignore]`d release-only test
(completes in under a second):

```sh
cargo test --release --test eval_four_bombs -- --ignored
```

### Eval coverage: 5-10 bomb boards (sampled + stratified)

Exhaustive enumeration is infeasible past k≈6 (`C(64,10) = 151,473,214,816`),
so [`tests/eval_multi_bomb.rs`](tests/eval_multi_bomb.rs) is a **sampled +
stratified** grid over `k = 5..=10`. Shared test helpers live in
[`tests/common/mod.rs`](tests/common/mod.rs).

- **Sampled invariants**: a deterministic pseudo-random sample
  (`20,000`/k for win/lose, `4,000`/k for the shielded strike ladder). Each
  board's win/lose line and the shielded ladder (strikes 1-2 survive, 3
  terminates with `k-3` bombs left) are asserted.
- **Curated layouts** reach adjacency counts **5-8** (random sampling almost
  never does), covering observation values `0.5 … 0.8`.
- **Coverage assertions**: the sample reaches depth-`≥5` adjacency, contains
  multi-region partitions (`zero_regions ≥ 2`), and has walled-off cells.
- **Deductive solvability**: [solvable-vs-must-guess][solver] classification
  using single-cell + subset rules, asserting both classes appear (the
  must-guess fraction grows with bomb count). A cross-check confirms every
  board the solver calls solvable really does win.
- **Golden trajectories** for the curated ring layouts and deterministic
  sampled representatives.

Exhaustive opt-in runs for k=5 and k=6 (release; ~5s and ~55s):

```sh
cargo test --release --test eval_multi_bomb -- --ignored
```

[solver]: https://en.wikipedia.org/wiki/Minesweeper_(video_game)#Strategy

## Scope & limitations

- This is a **single-board state machine**, not a full game front-end or a
  trained solver. It models one 8×8 board with `Reveal`/`Flag` actions.
- The crate is deliberately dependency-free and `no_std`; it has no heap and no
  I/O. Any policy training, ONNX runtime, or dataset pipeline belongs in a
  separate crate that depends on this one.
- ONNX policy evaluation is a documented use case but **no example ships in
  version 0.1.0**. Adding a feature-gated evaluation example is tracked as a
  follow-up.
- Bomb placement clamps `num_bombs` to 63 so a seeded board always has at least
  one safe cell. `from_mine_mask` accepts arbitrary masks, including the
  unwinnable full-mine fixture.
- Test coverage combines deterministic unit/eval suites with property-based
  invariant tests; exhaustive enumeration is opt-in for the larger bomb counts.

## License

Licensed under either of MIT or Apache-2.0, at your option.

