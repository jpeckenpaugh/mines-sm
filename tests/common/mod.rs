//! Shared helpers for the eval test suites.
//!
//! Included per test crate via `mod common;`. Not every helper is used by every
//! test file, hence the crate-level `allow(dead_code)`.
#![allow(dead_code)]

use mine_sm::{Action, ActionType, BoardState, Rules, StepResult};

// -----------------------------------------------------------------------------
// Actions and layout
// -----------------------------------------------------------------------------

pub fn bit(cell: u8) -> u64 {
    1u64 << cell
}

pub fn reveal(cell: u8) -> Action {
    Action {
        action_type: ActionType::Reveal,
        cell,
    }
}

pub fn flag(cell: u8) -> Action {
    Action {
        action_type: ActionType::Flag,
        cell,
    }
}

/// A classic-rules board pinned to `mines`.
pub fn board(mines: u64) -> BoardState {
    board_with(mines, Rules::default())
}

pub fn board_with(mines: u64, rules: Rules) -> BoardState {
    BoardState::from_mine_mask(mines, rules)
}

/// The lowest-index bomb cell.
pub fn first_bomb(mines: u64) -> u8 {
    mines.trailing_zeros() as u8
}

/// The (up to 8) neighbours of a cell, and how many there are.
pub fn neighbors(cell: u8) -> ([u8; 8], u8) {
    let mut out = [0u8; 8];
    let mut n = 0u8;
    let r = (cell / 8) as i8;
    let c = (cell % 8) as i8;
    for dr in -1..=1 {
        for dc in -1..=1 {
            if dr == 0 && dc == 0 {
                continue;
            }
            let (nr, nc) = (r + dr, c + dc);
            if (0..8).contains(&nr) && (0..8).contains(&nc) {
                out[n as usize] = (nr * 8 + nc) as u8;
                n += 1;
            }
        }
    }
    (out, n)
}

// -----------------------------------------------------------------------------
// Sampling
// -----------------------------------------------------------------------------

/// Deterministic SplitMix64, local to tests so sampling is reproducible.
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

/// A random layout with exactly `k` bombs.
pub fn random_mask(rng: &mut SplitMix64, k: u8) -> u64 {
    loop {
        let mut mines = 0u64;
        let mut placed = 0u8;
        while placed < k {
            let b = bit((rng.next() % 64) as u8);
            if mines & b == 0 {
                mines |= b;
                placed += 1;
            }
        }
        if mines.count_ones() == u32::from(k) {
            return mines;
        }
    }
}

/// A deterministic pseudo-random sample of `count` `k`-bomb layouts.
pub fn sample_masks(seed: u64, k: u8, count: u32) -> Vec<u64> {
    let mut rng = SplitMix64::new(seed ^ (u64::from(k) << 32));
    (0..count).map(|_| random_mask(&mut rng, k)).collect()
}

// -----------------------------------------------------------------------------
// Playing lines
// -----------------------------------------------------------------------------

/// Adaptive winning line: repeatedly open the lowest-index safe cell still
/// hidden. The generator knows the layout, so this never needs to guess.
pub fn play_win_with(mines: u64, rules: Rules) -> (BoardState, Vec<StepResult>) {
    let mut board = board_with(mines, rules);
    let mut results = Vec::new();
    while !board.is_terminal {
        let next = (0..64u8).find(|&c| {
            let b = bit(c);
            (mines & b) == 0 && (board.revealed_mask & b) == 0 && (board.flagged_mask & b) == 0
        });
        let Some(cell) = next else { break };
        results.push(board.step(reveal(cell)));
    }
    (board, results)
}

pub fn play_win(mines: u64) -> (BoardState, Vec<StepResult>) {
    play_win_with(mines, Rules::default())
}

/// Per-step `newly_revealed` counts for the winning line.
pub fn win_trace(mines: u64) -> Vec<u32> {
    play_win(mines)
        .1
        .iter()
        .map(|r| u32::from(r.newly_revealed))
        .collect()
}

/// Losing line: open a safe *numbered* cell first (cannot cascade), then
/// detonate the lowest-index bomb.
pub fn lose_line_with(mines: u64, rules: Rules) -> (BoardState, Vec<StepResult>) {
    let probe = board_with(mines, rules);
    let safe = (0..64u8)
        .find(|&c| (mines & bit(c)) == 0 && probe.adj_counts[c as usize] > 0)
        .expect("a board with bombs has a numbered neighbour");
    let bomb = first_bomb(mines);

    let mut board = board_with(mines, rules);
    let results = vec![board.step(reveal(safe)), board.step(reveal(bomb))];
    (board, results)
}

pub fn lose_line(mines: u64) -> (BoardState, Vec<StepResult>) {
    lose_line_with(mines, Rules::default())
}

// -----------------------------------------------------------------------------
// Feature extractors (used to assert coverage is meaningful)
// -----------------------------------------------------------------------------

/// The largest adjacent-bomb count anywhere on the board.
pub fn max_adjacency(mines: u64) -> u8 {
    board(mines).adj_counts.iter().copied().max().unwrap_or(0)
}

/// Number of connected components of zero-adjacency safe cells. More than one
/// means the board is partitioned into disjoint cascade regions.
pub fn zero_regions(mines: u64) -> u32 {
    let state = board(mines);
    let mut seen = 0u64;
    let mut regions = 0u32;
    for start in 0..64u8 {
        let b = bit(start);
        if seen & b != 0 || (mines & b) != 0 || state.adj_counts[start as usize] != 0 {
            continue;
        }
        regions += 1;
        let mut stack = vec![start];
        seen |= b;
        while let Some(curr) = stack.pop() {
            let (ns, count) = neighbors(curr);
            for &n in &ns[..count as usize] {
                let nb = bit(n);
                if seen & nb == 0 && (mines & nb) == 0 && state.adj_counts[n as usize] == 0 {
                    seen |= nb;
                    stack.push(n);
                }
            }
        }
    }
    regions
}

/// Safe numbered cells that touch no zero cell, so no cascade can ever reveal
/// them.
pub fn walled_cells(mines: u64) -> u32 {
    let state = board(mines);
    (0..64u8)
        .filter(|&c| {
            let cb = bit(c);
            if (mines & cb) != 0 || state.adj_counts[c as usize] == 0 {
                return false;
            }
            let (ns, count) = neighbors(c);
            !ns[..count as usize]
                .iter()
                .any(|&n| state.adj_counts[n as usize] == 0 && (mines & bit(n)) == 0)
        })
        .count() as u32
}

// -----------------------------------------------------------------------------
// Deductive solvability
// -----------------------------------------------------------------------------

/// True if the board can be cleared by pure deduction (single-cell and subset
/// rules) from *some* safe opening — i.e. without ever guessing.
///
/// The first move is treated as free (the engine guarantees it safe); we try
/// every safe opening and succeed if any clears the board.
pub fn solvable(mines: u64) -> bool {
    (0..64u8)
        .filter(|&c| (mines & bit(c)) == 0)
        .any(|opening| solvable_from(mines, opening))
}

fn solvable_from(mines: u64, opening: u8) -> bool {
    let mut board = board(mines);
    board.step(reveal(opening));
    let mut known_mines = 0u64;

    loop {
        if board.is_won {
            return true;
        }

        let mut constraints: Vec<(u64, u32)> = Vec::new();
        let mut forced_safe = 0u64;
        let mut forced_mine = 0u64;

        for c in 0..64u8 {
            let cb = bit(c);
            if board.revealed_mask & cb == 0 || mines & cb != 0 {
                continue;
            }
            let (ns, count) = neighbors(c);
            let mut unknown = 0u64;
            let mut known_adj = 0u32;
            for &n in &ns[..count as usize] {
                let nb = bit(n);
                if board.revealed_mask & nb != 0 {
                    continue;
                }
                if known_mines & nb != 0 {
                    known_adj += 1;
                } else {
                    unknown |= nb;
                }
            }
            if unknown == 0 {
                continue;
            }
            let remaining = u32::from(board.adj_counts[c as usize]).saturating_sub(known_adj);
            let ucount = unknown.count_ones();
            if remaining == 0 {
                forced_safe |= unknown;
            }
            if remaining == ucount {
                forced_mine |= unknown;
            }
            constraints.push((unknown, remaining));
        }

        // Subset rule: for A subset of B, the difference is forced.
        for i in 0..constraints.len() {
            for j in 0..constraints.len() {
                if i == j {
                    continue;
                }
                let (a, ra) = constraints[i];
                let (b, rb) = constraints[j];
                if a == b || a & !b != 0 {
                    continue;
                }
                let diff = b & !a;
                let dr = rb as i32 - ra as i32;
                if dr == 0 {
                    forced_safe |= diff;
                } else if dr >= 0 && dr as u32 == diff.count_ones() {
                    forced_mine |= diff;
                }
            }
        }

        let mut progress = false;

        let new_mines = forced_mine & !known_mines & !board.revealed_mask;
        if new_mines != 0 {
            known_mines |= new_mines;
            progress = true;
        }

        let to_open = forced_safe & !board.revealed_mask & !known_mines;
        if to_open != 0 {
            for c in 0..64u8 {
                if to_open & bit(c) == 0 {
                    continue;
                }
                let r = board.step(reveal(c));
                if r.hit_bomb {
                    return false; // deduction was wrong (shouldn't happen)
                }
            }
            progress = true;
        }

        if !progress {
            return board.is_won;
        }
    }
}
