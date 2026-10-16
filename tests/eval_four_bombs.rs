//! Coverage for 4-bomb boards.
//!
//! `C(64, 4) = 635376` layouts — too many to enumerate in the default test run,
//! so this file uses a **1% stride sample** (every 100th layout, ~6354 boards)
//! with a winning and losing line each (`~12708` fixtures), plus a curated set
//! of structured layouts that force the behaviors uniform sampling only hits by
//! luck: 4-bomb adjacency counts, 2x2 clusters, and multi-region cascades.
//!
//! The full enumeration is available as an `#[ignore]`d, release-only test:
//!
//! ```sh
//! cargo test --release --test eval_four_bombs -- --ignored
//! ```

mod common;

use common::{bit, first_bomb, lose_line, play_win, reveal, win_trace};
use mine_sm::{BoardState, Rules};

/// 1% stride over all 4-combinations. Deterministic and evenly spread.
fn sampled_layouts() -> Vec<u64> {
    let mut masks = Vec::new();
    let mut index: u32 = 0;
    for a in 0..64u8 {
        for b in (a + 1)..64u8 {
            for c in (b + 1)..64u8 {
                for d in (c + 1)..64u8 {
                    if index.is_multiple_of(100) {
                        masks.push(bit(a) | bit(b) | bit(c) | bit(d));
                    }
                    index += 1;
                }
            }
        }
    }
    assert_eq!(index, 635_376, "C(64,4)");
    masks
}

// -----------------------------------------------------------------------------
// Sampled grid
// -----------------------------------------------------------------------------

#[test]
fn win_eval_for_sampled_four_bomb_layouts() {
    for mines in sampled_layouts() {
        let (board, results) = play_win(mines);
        assert!(results.iter().all(|r| r.valid_move), "all moves valid");
        assert!(
            !results.iter().any(|r| r.hit_bomb),
            "no detonation on a win"
        );
        let total: u32 = results.iter().map(|r| u32::from(r.newly_revealed)).sum();
        assert_eq!(total, 60, "60 safe cells revealed once each");
        assert!(board.is_terminal && board.is_won);
        assert_eq!(board.revealed_mask, !mines);
        assert_eq!(board.strikes, 0);
    }
}

#[test]
fn lose_eval_for_sampled_four_bomb_layouts() {
    for mines in sampled_layouts() {
        let bomb = first_bomb(mines);
        let (board, results) = lose_line(mines);
        assert_eq!(results.len(), 2);
        assert!(!results[0].hit_bomb && !results[0].terminal);
        assert!(results[1].hit_bomb && results[1].terminal && !results[1].won);
        assert!(board.is_terminal && !board.is_won);
        assert_eq!(board.strikes, 1);
        assert_eq!(board.detonated_mask & bit(bomb), bit(bomb));
        // The board still carries all four bombs' worth of danger.
        assert_eq!(board.mine_mask.count_ones(), 4);
    }
}

// -----------------------------------------------------------------------------
// Coverage assertions — is the sample meaningful?
// -----------------------------------------------------------------------------

#[test]
fn sample_size_is_about_one_percent() {
    let masks = sampled_layouts();
    assert_eq!(masks.len(), 6354, "1% stride of C(64,4)");
    assert!(masks.iter().all(|m| m.count_ones() == 4));
}

#[test]
fn sample_exercises_full_adjacency_range_up_to_four() {
    let mut saw_four = 0;
    for mines in sampled_layouts() {
        let board = BoardState::from_mine_mask(mines, Rules::default());
        if board.adj_counts.contains(&4) {
            saw_four += 1;
        }
    }
    assert!(saw_four > 0, "expected a cell adjacent to all 4 bombs");
}

#[test]
fn sample_covers_single_and_multi_step_wins() {
    let masks = sampled_layouts();
    let one_shot = masks.iter().filter(|&&m| win_trace(m).len() == 1).count();
    let multi = masks.len() - one_shot;
    assert!(one_shot > 0, "expected some single-cascade wins");
    assert!(multi > 0, "expected some multi-region / multi-step wins");
}

#[test]
fn sample_places_bombs_in_every_cell_class() {
    let corner = |c: u8| matches!(c, 0 | 7 | 56 | 63);
    let edge = |c: u8| {
        let (r, col) = (c / 8, c % 8);
        (r == 0 || r == 7 || col == 0 || col == 7) && !corner(c)
    };
    let interior = |c: u8| {
        let (r, col) = (c / 8, c % 8);
        r != 0 && r != 7 && col != 0 && col != 7
    };

    let (mut has_corner, mut has_edge, mut has_interior) = (false, false, false);
    for mines in sampled_layouts() {
        for c in 0..64u8 {
            if mines & bit(c) != 0 {
                has_corner |= corner(c);
                has_edge |= edge(c);
                has_interior |= interior(c);
            }
        }
    }
    assert!(
        has_corner && has_edge && has_interior,
        "all bomb classes covered"
    );
}

// -----------------------------------------------------------------------------
// Structured layouts + golden trajectories
// -----------------------------------------------------------------------------

/// `(label, mine_mask)` for configurations chosen to force specific behavior.
fn structured_cases() -> Vec<(&'static str, u64)> {
    let diag = bit(0) | bit(9) | bit(18) | bit(27);
    vec![
        ("2x2 block", bit(0) | bit(1) | bit(8) | bit(9)),
        ("diagonal line", diag),
        ("top edge row", bit(0) | bit(1) | bit(2) | bit(3)),
        ("four corners", bit(0) | bit(7) | bit(56) | bit(63)),
        ("center count-4", bit(18) | bit(19) | bit(26) | bit(34)),
        ("opposite pairs", bit(0) | bit(9) | bit(54) | bit(63)),
    ]
}

#[test]
fn structured_layouts_win_and_lose() {
    for (label, mines) in structured_cases() {
        let (win_board, win_results) = play_win(mines);
        assert!(win_board.is_won, "{label}: winnable");
        let total: u32 = win_results
            .iter()
            .map(|r| u32::from(r.newly_revealed))
            .sum();
        assert_eq!(total, 60, "{label}");

        let (lose_board, _) = lose_line(mines);
        assert!(
            lose_board.is_terminal && !lose_board.is_won,
            "{label}: losable"
        );
    }
}

#[test]
fn center_count_four_encodes_observation() {
    // Cell 27 is adjacent to all four bombs (18, 19, 26, 34) -> count 4 -> 0.4.
    let mines = bit(18) | bit(19) | bit(26) | bit(34);
    let mut board = BoardState::from_mine_mask(mines, Rules::default());
    assert_eq!(board.adj_counts[27], 4);

    let r = board.step(reveal(27));
    assert!(r.valid_move && !r.hit_bomb);
    assert_eq!(r.newly_revealed, 1, "a numbered cell does not cascade");

    let mut obs = [0f32; 64];
    board.get_observation(&mut obs);
    assert!((obs[27] - 0.4).abs() < 1e-6, "count 4 encodes as 0.4");
}

#[test]
fn golden_structured_trajectories() {
    for (label, mines) in structured_cases() {
        let trace = win_trace(mines);
        let sum: u32 = trace.iter().sum();
        assert_eq!(sum, 60, "{label}: all safe cells accounted for");
        assert!(!trace.is_empty(), "{label}");
    }

    // Baked exact traces for the structured representatives.
    assert_eq!(win_trace(bit(0) | bit(1) | bit(8) | bit(9)), vec![1, 59]);
    assert_eq!(
        win_trace(bit(0) | bit(9) | bit(18) | bit(27)),
        vec![1, 1, 57, 1]
    );
    assert_eq!(win_trace(bit(0) | bit(7) | bit(56) | bit(63)), vec![1, 59]);
}

// -----------------------------------------------------------------------------
// Exhaustive (opt-in)
// -----------------------------------------------------------------------------

#[test]
#[ignore = "enumerates all 635376 layouts; run in release"]
fn exhaustive_four_bomb_wins() {
    let mut count: u64 = 0;
    for a in 0..64u8 {
        for b in (a + 1)..64u8 {
            for c in (b + 1)..64u8 {
                for d in (c + 1)..64u8 {
                    let mines = bit(a) | bit(b) | bit(c) | bit(d);
                    let (board, _) = play_win(mines);
                    assert!(board.is_won, "({a},{b},{c},{d})");
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 635_376);
}
