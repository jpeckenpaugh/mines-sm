//! Exhaustive eval coverage over the 64 single-bomb boards.
//!
//! For each bomb position (`0..64`) we run two scripted games on a classic
//! (`max_strikes = 1`) board:
//!
//! * a **winning** line that must reveal all 63 safe cells without detonating,
//! * a **losing** line that must detonate the single bomb and terminate.
//!
//! That is 64 win + 64 lose = 128 deterministic fixtures. The outcome axis
//! covers the two terminal states; the position axis covers every board
//! geometry (corner, edge, interior).
//!
//! Layouts are pinned with `BoardState::from_mine_mask`, so a fixture names an
//! exact bomb position rather than hunting for a seed.

mod common;

use common::{bit, lose_line, play_win, win_trace};

// -----------------------------------------------------------------------------
// Win evals — one per bomb position
// -----------------------------------------------------------------------------

#[test]
fn win_eval_for_every_single_bomb_position() {
    for pos in 0..64u8 {
        let mines = bit(pos);
        let (board, results) = play_win(mines);

        // Every executed action was legal and none detonated.
        assert!(
            results.iter().all(|r| r.valid_move),
            "pos {pos}: all moves valid"
        );
        assert!(
            !results.iter().any(|r| r.hit_bomb),
            "pos {pos}: no bomb detonated on a win"
        );

        // Every safe cell is revealed exactly once across the whole line.
        let total_revealed: u32 = results.iter().map(|r| u32::from(r.newly_revealed)).sum();
        assert_eq!(
            total_revealed, 63,
            "pos {pos}: safe cells revealed once each"
        );

        // Terminal win state.
        assert!(board.is_terminal && board.is_won, "pos {pos}: won");
        assert_eq!(board.revealed_mask, !mines, "pos {pos}: full reveal");
        assert_eq!(board.detonated_mask, 0, "pos {pos}: nothing detonated");
        assert_eq!(board.flagged_mask, 0, "pos {pos}: no flags in win line");
        assert_eq!(board.strikes, 0, "pos {pos}: no strikes");

        // The final observed result announces the win.
        let last = results.last().expect("at least one move");
        assert!(last.terminal && last.won, "pos {pos}: last result is a win");
    }
}

#[test]
fn winning_lines_never_detonate_or_flag() {
    for pos in 0..64u8 {
        let (board, _) = play_win(bit(pos));
        assert_eq!(board.revealed_mask & board.mine_mask, 0, "pos {pos}");
        assert_eq!(board.revealed_mask.count_ones(), 63, "pos {pos}");
        assert_eq!(board.flagged_mask, 0, "pos {pos}");
    }
}

// -----------------------------------------------------------------------------
// Lose evals — one per bomb position
// -----------------------------------------------------------------------------

#[test]
fn lose_eval_for_every_single_bomb_position() {
    for pos in 0..64u8 {
        let (board, results) = lose_line(bit(pos));
        assert_eq!(results.len(), 2, "pos {pos}: expected exactly two moves");

        // Move 1: a plain, safe single reveal that does not end the game.
        let first = results[0];
        assert!(first.valid_move, "pos {pos}");
        assert!(!first.hit_bomb, "pos {pos}");
        assert_eq!(
            first.newly_revealed, 1,
            "pos {pos}: numbered cell reveals once"
        );
        assert!(
            !first.terminal,
            "pos {pos}: not terminal after one safe reveal"
        );

        // Move 2: detonation of the single bomb under classic rules.
        let second = results[1];
        assert!(second.valid_move, "pos {pos}");
        assert!(second.hit_bomb, "pos {pos}: bomb detonates");
        assert_eq!(second.newly_revealed, 1, "pos {pos}");
        assert!(second.terminal && !second.won, "pos {pos}: loss terminates");

        // Terminal loss state.
        assert!(board.is_terminal && !board.is_won, "pos {pos}");
        assert_eq!(board.strikes, 1, "pos {pos}: one strike");
        assert_eq!(board.detonated_mask & bit(pos), bit(pos), "pos {pos}");
        assert_eq!(board.flagged_mask, 0, "pos {pos}");
    }
}

// -----------------------------------------------------------------------------
// Coverage bookkeeping
// -----------------------------------------------------------------------------

#[test]
fn coverage_is_exhaustive_over_single_bomb_boards() {
    // Sanity check that the grids above really span all 64 positions for both
    // outcomes: 64 win + 64 lose = 128 scenarios.
    let positions: Vec<u8> = (0..64u8).collect();
    assert_eq!(positions.len(), 64);

    let mut scenarios = 0;
    for &pos in &positions {
        let (win_board, _) = play_win(bit(pos));
        assert!(win_board.is_won);
        scenarios += 1;

        let (lose_board, _) = lose_line(bit(pos));
        assert!(lose_board.is_terminal && !lose_board.is_won);
        scenarios += 1;
    }
    assert_eq!(scenarios, 128);
}

// -----------------------------------------------------------------------------
// Golden trajectories
// -----------------------------------------------------------------------------

#[test]
fn golden_win_trajectories_by_geometry() {
    // Exact per-step reveal counts pin down the cascade flood-fill *shape*, not
    // just the final outcome.
    //
    // * pos 3 and pos 27: the first safe cell (0) is a zero cell whose region
    //   spans the whole board, so one cascade wins.
    // * pos 0 (bomb in a corner): cell 1 borders the bomb so it opens alone,
    //   then cell 2's zero region floods the remaining 62 safe cells.
    // * pos 1 (bomb adjacent to the corner): the cascade is *partial* — some
    //   cells are walled off behind the bomb's numbered neighbours and take
    //   extra reveals. This is why not every board wins on the first click.
    assert_eq!(win_trace(bit(3)), vec![63]);
    assert_eq!(win_trace(bit(27)), vec![63]);
    assert_eq!(win_trace(bit(0)), vec![1, 62]);
    assert_eq!(win_trace(bit(1)), vec![1, 1, 61]);

    // Every win trace sums to all 63 safe cells.
    for pos in [0u8, 1, 3, 27] {
        let sum: u32 = win_trace(bit(pos)).iter().sum();
        assert_eq!(sum, 63, "pos {pos}: trace must account for every safe cell");
    }
}

#[test]
fn golden_lose_trajectory_is_two_single_reveals() {
    // The losing line is geometry-independent for single-bomb boards.
    for pos in [0u8, 1, 3, 27, 63] {
        let (board, results) = lose_line(bit(pos));
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].newly_revealed, 1);
        assert_eq!(results[1].newly_revealed, 1);
        assert_eq!(board.strikes, 1);
        assert!(board.is_terminal && !board.is_won);
    }
}
