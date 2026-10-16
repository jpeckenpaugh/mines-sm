//! Coverage for 5-10 bomb boards.
//!
//! Exhaustive enumeration is infeasible here (`C(64,10) = 151,473,214,816`), so
//! this is a **sampled + stratified** grid:
//!
//! * a deterministic pseudo-random sample per bomb count for win/lose/shielded
//!   invariants,
//! * feature assertions that the sample is meaningful (adjacency depth,
//!   multi-region partitioning, walled-off cells),
//! * curated layouts that reach adjacency counts 5-8 (random sampling almost
//!   never does),
//! * a deductive-solvability classification (solvable vs must-guess),
//! * opt-in exhaustive runs for k=5 and k=6:
//!
//! ```sh
//! cargo test --release --test eval_multi_bomb -- --ignored
//! ```

mod common;

use common::{
    bit, board, board_with, lose_line, max_adjacency, play_win, reveal, sample_masks, solvable,
    walled_cells, win_trace, zero_regions,
};
use mine_sm::{BoardState, Rules, StepResult};

/// Samples for the cheap invariant tests.
const SAMPLE: u32 = 20_000;
/// Smaller samples for the (more expensive) solver and shielded tests.
const SOLVER_SAMPLE: u32 = 500;
const SHIELDED_SAMPLE: u32 = 4_000;
/// Fixed sampling seed (reproducible).
const SEED: u64 = 0x00C0_FFEE_5EED;

const BOMB_COUNTS: [u8; 6] = [5, 6, 7, 8, 9, 10];

const SHIELDED: Rules = Rules { max_strikes: 3 };

/// The `n` neighbours of cell 27 (top-left first), so a mask of the first `n`
/// gives cell 27 an adjacency count of exactly `n`.
fn ring_mask(n: usize) -> u64 {
    let ring = [18u8, 19, 20, 26, 28, 34, 35, 36];
    ring[..n].iter().fold(0u64, |m, &c| m | bit(c))
}

/// Safe numbered open, then detonate three distinct bombs under shielded rules.
fn shielded_strike_line(mines: u64) -> (BoardState, Vec<StepResult>) {
    let probe = board_with(mines, SHIELDED);
    let safe = (0..64u8)
        .find(|&c| (mines & bit(c)) == 0 && probe.adj_counts[c as usize] > 0)
        .expect("numbered neighbour");
    let bombs: Vec<u8> = (0..64u8).filter(|&c| mines & bit(c) != 0).take(3).collect();

    let mut board = board_with(mines, SHIELDED);
    let mut results = vec![board.step(reveal(safe))];
    for &b in &bombs {
        results.push(board.step(reveal(b)));
    }
    (board, results)
}

// -----------------------------------------------------------------------------
// Sampled invariants
// -----------------------------------------------------------------------------

#[test]
fn win_eval_for_sampled_multi_bomb_layouts() {
    for k in BOMB_COUNTS {
        for mines in sample_masks(SEED, k, SAMPLE) {
            let (board, results) = play_win(mines);
            assert!(results.iter().all(|r| r.valid_move), "k={k}");
            assert!(!results.iter().any(|r| r.hit_bomb), "k={k}");
            let total: u32 = results.iter().map(|r| u32::from(r.newly_revealed)).sum();
            assert_eq!(total, 64 - u32::from(k), "k={k}: safe cells revealed once");
            assert!(board.is_terminal && board.is_won, "k={k}");
            assert_eq!(board.revealed_mask, !mines, "k={k}");
            assert_eq!(board.strikes, 0, "k={k}");
        }
    }
}

#[test]
fn lose_eval_for_sampled_multi_bomb_layouts() {
    for k in BOMB_COUNTS {
        for mines in sample_masks(SEED, k, SAMPLE) {
            let (board, results) = lose_line(mines);
            assert_eq!(results.len(), 2, "k={k}");
            assert!(!results[0].hit_bomb && !results[0].terminal, "k={k}");
            assert!(
                results[1].hit_bomb && results[1].terminal && !results[1].won,
                "k={k}"
            );
            assert!(board.is_terminal && !board.is_won, "k={k}");
            assert_eq!(board.strikes, 1, "k={k}");
        }
    }
}

#[test]
fn shielded_ladder_for_sampled_multi_bomb_layouts() {
    for k in BOMB_COUNTS {
        for mines in sample_masks(SEED, k, SHIELDED_SAMPLE) {
            let (board, results) = shielded_strike_line(mines);
            assert_eq!(results.len(), 4, "k={k}");
            assert!(!results[0].hit_bomb && !results[0].terminal, "k={k}");
            assert!(results[1].hit_bomb && !results[1].terminal, "k={k}");
            assert!(results[2].hit_bomb && !results[2].terminal, "k={k}");
            assert!(
                results[3].hit_bomb && results[3].terminal && !results[3].won,
                "k={k}"
            );
            assert_eq!(board.strikes, SHIELDED.max_strikes, "k={k}");
            assert_eq!(
                (board.mine_mask & !board.detonated_mask).count_ones(),
                u32::from(k) - 3,
                "k={k}: bombs remaining after terminal"
            );
        }
    }
}

// -----------------------------------------------------------------------------
// Feature coverage — is the sample meaningful?
// -----------------------------------------------------------------------------

#[test]
fn sample_reaches_depth_adjacency_and_partitioning() {
    for k in BOMB_COUNTS {
        let masks = sample_masks(SEED, k, SAMPLE);

        let deepest = masks.iter().map(|&m| max_adjacency(m)).max().unwrap();
        assert!(
            deepest >= 5,
            "k={k}: expected adjacency >= 5, got {deepest}"
        );

        let partitioned = masks.iter().filter(|&&m| zero_regions(m) >= 2).count();
        assert!(partitioned > 0, "k={k}: expected some multi-region boards");

        let walled = masks.iter().filter(|&&m| walled_cells(m) > 0).count();
        assert!(walled > 0, "k={k}: expected some walled-off cells");
    }
}

#[test]
fn curated_layouts_reach_adjacency_five_through_eight() {
    for n in 5..=8usize {
        let mines = ring_mask(n);
        assert_eq!(board(mines).adj_counts[27], n as u8, "ring {n}");

        // Winnable and losable like any other board.
        assert!(play_win(mines).0.is_won, "ring {n}: winnable");
        assert!(lose_line(mines).0.is_terminal, "ring {n}: losable");

        // Observation encoding covers the upper range 0.5 ..= 0.8.
        let mut b = board(mines);
        let r = b.step(reveal(27));
        assert!(
            r.valid_move && !r.hit_bomb && r.newly_revealed == 1,
            "ring {n}"
        );
        let mut obs = [0f32; 64];
        b.get_observation(&mut obs);
        assert!(
            (obs[27] - n as f32 / 10.0).abs() < 1e-6,
            "ring {n} encoding"
        );
    }
}

// -----------------------------------------------------------------------------
// Deductive solvability: solvable vs must-guess
// -----------------------------------------------------------------------------

#[test]
fn sample_contains_both_solvable_and_must_guess_boards() {
    for k in BOMB_COUNTS {
        let masks = sample_masks(SEED ^ 0x5A17, k, SOLVER_SAMPLE);
        let solvable_count = masks.iter().filter(|&&m| solvable(m)).count();
        let must_guess = masks.len() - solvable_count;

        assert!(solvable_count > 0, "k={k}: expected some solvable boards");
        assert!(
            must_guess > 0,
            "k={k}: expected some boards requiring a guess"
        );
    }
}

#[test]
fn solvable_boards_can_actually_be_cleared() {
    // Cross-check: every board the solver calls solvable really does win.
    for k in [6u8, 8, 10] {
        let masks = sample_masks(SEED ^ 0x5A17, k, SOLVER_SAMPLE);
        let checked = masks.iter().filter(|&&m| solvable(m)).take(50);
        let mut count = 0;
        for &mines in checked {
            let (board, _) = common::play_win(mines);
            assert!(board.is_won, "solver claimed solvable but board not won");
            count += 1;
        }
        assert!(count > 0, "k={k}: expected some solvable boards to check");
    }
}

// -----------------------------------------------------------------------------
// Exhaustive (opt-in, release only)
// -----------------------------------------------------------------------------

#[test]
#[ignore = "enumerates all 7,624,512 5-bomb layouts; run in release"]
fn exhaustive_five_bomb_wins() {
    let mut count: u64 = 0;
    for a in 0..64u8 {
        for b in (a + 1)..64u8 {
            for c in (b + 1)..64u8 {
                for d in (c + 1)..64u8 {
                    for e in (d + 1)..64u8 {
                        let mines = bit(a) | bit(b) | bit(c) | bit(d) | bit(e);
                        assert!(play_win(mines).0.is_won);
                        count += 1;
                    }
                }
            }
        }
    }
    assert_eq!(count, 7_624_512);
}

#[test]
#[ignore = "enumerates all 74,974,368 6-bomb layouts; run in release"]
fn exhaustive_six_bomb_wins() {
    let mut count: u64 = 0;
    for a in 0..64u8 {
        for b in (a + 1)..64u8 {
            for c in (b + 1)..64u8 {
                for d in (c + 1)..64u8 {
                    for e in (d + 1)..64u8 {
                        for f in (e + 1)..64u8 {
                            let mines = bit(a) | bit(b) | bit(c) | bit(d) | bit(e) | bit(f);
                            assert!(play_win(mines).0.is_won);
                            count += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(count, 74_974_368);
}

// -----------------------------------------------------------------------------
// Golden trajectories
// -----------------------------------------------------------------------------

#[test]
fn golden_multi_bomb_trajectories() {
    // Curated ring layouts: cell 0's zero region floods all but one walled cell,
    // which needs a final lone reveal.
    assert_eq!(win_trace(ring_mask(5)), vec![58, 1]);
    assert_eq!(win_trace(ring_mask(6)), vec![57, 1]);
    assert_eq!(win_trace(ring_mask(7)), vec![56, 1]);
    assert_eq!(win_trace(ring_mask(8)), vec![55, 1]);

    // Deterministic sampled representatives (fixed seed) lock the cascade shape.
    assert_eq!(
        win_trace(sample_masks(SEED, 5, 1)[0]),
        vec![1, 1, 1, 1, 11, 1, 1, 1, 40, 1]
    );
    assert_eq!(
        win_trace(sample_masks(SEED, 10, 1)[0]),
        vec![38, 1, 1, 1, 1, 1, 1, 1, 8, 1]
    );

    // Every consumed trace accounts for exactly the safe cells.
    for k in BOMB_COUNTS {
        let mines = sample_masks(SEED, k, 1)[0];
        let sum: u32 = win_trace(mines).iter().sum();
        assert_eq!(sum, 64 - u32::from(k), "k={k}");
    }
}
