//! Exhaustive eval coverage over the 2-bomb boards.
//!
//! There are `C(64, 2) = 2016` distinct 2-bomb layouts, so this extends the
//! single-bomb grid in `eval_scenarios.rs` one bomb deep. Unlike the 1-bomb
//! case, two bombs can split the board into multiple zero-regions, which is the
//! cascade shape the single-bomb set cannot produce.
//!
//! For each layout we run a winning line (reveal all 62 safe cells) and a
//! losing line (detonate a bomb), both under classic rules: 2016 + 2016 = 4032
//! deterministic fixtures.

mod common;

use common::{bit, lose_line, play_win, win_trace};

/// All unordered bomb pairs on the 64-cell board: `C(64, 2) = 2016`.
fn all_pairs() -> Vec<(u8, u8)> {
    let mut pairs = Vec::with_capacity(2016);
    for a in 0..64u8 {
        for b in (a + 1)..64u8 {
            pairs.push((a, b));
        }
    }
    pairs
}

// -----------------------------------------------------------------------------
// Win evals — one per 2-bomb layout
// -----------------------------------------------------------------------------

#[test]
fn win_eval_for_every_two_bomb_layout() {
    let pairs = all_pairs();
    assert_eq!(pairs.len(), 2016, "C(64,2)");

    for (a, b) in pairs {
        let mines = bit(a) | bit(b);
        let (board, results) = play_win(mines);

        assert!(
            results.iter().all(|r| r.valid_move),
            "({a},{b}): all moves valid"
        );
        assert!(
            !results.iter().any(|r| r.hit_bomb),
            "({a},{b}): no detonation on a win"
        );

        let total: u32 = results.iter().map(|r| u32::from(r.newly_revealed)).sum();
        assert_eq!(total, 62, "({a},{b}): 62 safe cells revealed once each");

        assert!(board.is_terminal && board.is_won, "({a},{b}): won");
        assert_eq!(board.revealed_mask, !mines, "({a},{b}): full reveal");
        assert_eq!(board.detonated_mask, 0, "({a},{b})");
        assert_eq!(board.flagged_mask, 0, "({a},{b})");
        assert_eq!(board.strikes, 0, "({a},{b})");

        let last = results.last().expect("at least one move");
        assert!(last.terminal && last.won, "({a},{b})");
    }
}

// -----------------------------------------------------------------------------
// Lose evals — one per 2-bomb layout
// -----------------------------------------------------------------------------

#[test]
fn lose_eval_for_every_two_bomb_layout() {
    for (a, b) in all_pairs() {
        let mines = bit(a) | bit(b);
        let bomb = mines.trailing_zeros() as u8;
        let (board, results) = lose_line(mines);

        assert_eq!(results.len(), 2, "({a},{b}): two moves");
        assert!(results[0].valid_move && !results[0].hit_bomb, "({a},{b})");
        assert_eq!(results[0].newly_revealed, 1, "({a},{b}): numbered cell");
        assert!(!results[0].terminal, "({a},{b})");

        assert!(results[1].hit_bomb && results[1].terminal, "({a},{b})");
        assert!(!results[1].won, "({a},{b})");

        assert!(board.is_terminal && !board.is_won, "({a},{b})");
        assert_eq!(board.strikes, 1, "({a},{b})");
        assert_eq!(board.detonated_mask & bit(bomb), bit(bomb), "({a},{b})");
        assert_eq!(board.flagged_mask, 0, "({a},{b})");
    }
}

#[test]
fn coverage_is_exhaustive_over_two_bomb_boards() {
    // 2016 win + 2016 lose = 4032 scenarios, with overlapping bomb pairs being
    // the only duplicates by construction.
    let mut win = 0u32;
    let mut lose = 0u32;
    for (a, b) in all_pairs() {
        let mines = bit(a) | bit(b);
        assert!(play_win(mines).0.is_won);
        win += 1;
        assert!(lose_line(mines).0.is_terminal && !lose_line(mines).0.is_won);
        lose += 1;
    }
    assert_eq!(win, 2016);
    assert_eq!(lose, 2016);
    assert_eq!(win + lose, 4032);
}

// -----------------------------------------------------------------------------
// Non-degeneracy: two bombs can create partial / multi-region cascades
// -----------------------------------------------------------------------------

#[test]
fn two_bomb_boards_are_not_all_single_cascade_wins() {
    // On a 1-bomb board many positions flood the whole board from the first
    // safe cell. Two bombs can split the zero-region so this no longer holds;
    // assert that some boards genuinely need more than one reveal to win.
    let pairs = all_pairs();
    let one_shot = pairs
        .iter()
        .filter(|&&(a, b)| win_trace(bit(a) | bit(b)).len() == 1)
        .count();
    let multi = pairs.len() - one_shot;

    assert!(multi > 0, "expected some multi-step win lines, found none");
    assert!(
        one_shot > 0,
        "expected some single-cascade wins, found none"
    );
}

// -----------------------------------------------------------------------------
// Geometry bookkeeping: 278 canonical layouts up to D4 symmetry
// -----------------------------------------------------------------------------

#[test]
fn two_bomb_layouts_have_278_symmetry_classes() {
    // Burnside over the 8 symmetries of the square. Confirms the raw 2016
    // layouts collapse to 278 canonical geometries.
    // The 8 symmetries of the square, as plain `fn` pointers over (row, col).
    type Sym = fn((i32, i32)) -> (i32, i32);
    fn ident(p: (i32, i32)) -> (i32, i32) {
        p
    }
    fn r90((r, c): (i32, i32)) -> (i32, i32) {
        (c, 7 - r)
    }
    fn r180((r, c): (i32, i32)) -> (i32, i32) {
        (7 - r, 7 - c)
    }
    fn r270((r, c): (i32, i32)) -> (i32, i32) {
        (7 - c, r)
    }
    fn refl_v((r, c): (i32, i32)) -> (i32, i32) {
        (r, 7 - c)
    }
    fn refl_h((r, c): (i32, i32)) -> (i32, i32) {
        (7 - r, c)
    }
    fn refl_d((r, c): (i32, i32)) -> (i32, i32) {
        (c, r)
    }
    fn refl_a((r, c): (i32, i32)) -> (i32, i32) {
        (7 - c, 7 - r)
    }

    let group: [Sym; 8] = [ident, r90, r180, r270, refl_v, refl_h, refl_d, refl_a];

    let to_rc = |cell: u8| ((cell / 8) as i32, (cell % 8) as i32);
    let to_cell = |(r, c): (i32, i32)| (r * 8 + c) as u8;

    let pairs = all_pairs();
    let mut burnside: u32 = 0;
    for g in group {
        let fixed = pairs
            .iter()
            .filter(|&&(a, b)| {
                let (ra, rb) = (to_cell(g(to_rc(a))), to_cell(g(to_rc(b))));
                let (lo, hi) = if ra < rb { (ra, rb) } else { (rb, ra) };
                (lo, hi) == (a, b)
            })
            .count() as u32;
        burnside += fixed;
    }

    assert_eq!(burnside % 8, 0);
    assert_eq!(burnside / 8, 278);
}

// -----------------------------------------------------------------------------
// Golden trajectories for representative layouts
// -----------------------------------------------------------------------------

#[test]
fn golden_two_bomb_trajectories() {
    // Adjacent pair in the corner: cell 1 borders a bomb so it opens alone, then
    // the zero-region floods the remaining 61 safe cells.
    // Diagonal-from-corner pair: the bombs pinch the board enough that the
    // cascade stays partial across several reveals (a real multi-region case).
    // Opposite corners: cell 1 opens alone, then one flood fills the rest.
    assert_eq!(win_trace(bit(0) | bit(1)), vec![1, 61]);
    assert_eq!(win_trace(bit(0) | bit(9)), vec![1, 1, 59, 1]);
    assert_eq!(win_trace(bit(0) | bit(63)), vec![1, 61]);

    for (a, b) in [(0u8, 1u8), (0, 9), (0, 63)] {
        let sum: u32 = win_trace(bit(a) | bit(b)).iter().sum();
        assert_eq!(sum, 62, "({a},{b}): all safe cells accounted for");
    }
}
