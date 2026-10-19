//! Integration tests for the `mine-sm` Minesweeper state machine.
//!
//! These exercise the public API only (`BoardState`, `Action`, `ActionType`,
//! `Rules`) against the observable layer and the documented invariants.

use mine_sm::{Action, ActionType, BoardState, Rules};

// -----------------------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------------------

fn reveal(cell: u8) -> Action {
    Action {
        action_type: ActionType::Reveal,
        cell,
    }
}

fn flag(cell: u8) -> Action {
    Action {
        action_type: ActionType::Flag,
        cell,
    }
}

fn bit(cell: u8) -> u64 {
    1u64 << cell
}

fn bombs(board: &BoardState) -> u32 {
    board.mine_mask.count_ones()
}

fn first_bomb(board: &BoardState) -> u8 {
    (0..64u8)
        .find(|&i| (board.mine_mask & bit(i)) != 0)
        .expect("board should contain at least one bomb")
}

fn all_bombs(board: &BoardState) -> Vec<u8> {
    (0..64u8)
        .filter(|&i| (board.mine_mask & bit(i)) != 0)
        .collect()
}

fn first_safe(board: &BoardState) -> u8 {
    (0..64u8)
        .find(|&i| (board.mine_mask & bit(i)) == 0)
        .expect("board should contain at least one safe cell")
}

fn first_safe_with_count(board: &BoardState, min: u8) -> u8 {
    (0..64u8)
        .find(|&i| {
            (board.mine_mask & bit(i)) == 0
                && (board.flagged_mask & bit(i)) == 0
                && board.adj_counts[i as usize] >= min
        })
        .expect("expected a safe cell meeting the adjacency requirement")
}

/// Recompute a cell's adjacent-bomb count independently of the implementation.
fn expected_adj(board: &BoardState, cell: u8) -> u8 {
    let r = (cell / 8) as i32;
    let c = (cell % 8) as i32;
    let mut count = 0u8;
    for dr in -1..=1 {
        for dc in -1..=1 {
            if dr == 0 && dc == 0 {
                continue;
            }
            let nr = r + dr;
            let nc = c + dc;
            if (0..8).contains(&nr) && (0..8).contains(&nc) {
                let n = (nr * 8 + nc) as u8;
                if (board.mine_mask & bit(n)) != 0 {
                    count += 1;
                }
            }
        }
    }
    count
}

// -----------------------------------------------------------------------------
// Construction & determinism
// -----------------------------------------------------------------------------

#[test]
fn same_seed_produces_identical_layout() {
    let a = BoardState::new(42, 10, Rules::default());
    let b = BoardState::new(42, 10, Rules::default());
    assert_eq!(a.mine_mask, b.mine_mask);
    assert_eq!(a.adj_counts, b.adj_counts);
}

#[test]
fn seed_changes_layout() {
    // Two fixed, distinct seeds that are known to differ for 10 bombs.
    let a = BoardState::new(1, 10, Rules::default());
    let b = BoardState::new(2, 10, Rules::default());
    assert_ne!(a.mine_mask, b.mine_mask);
}

#[test]
fn exact_bomb_count_is_placed() {
    for num_bombs in [0u8, 1, 5, 10, 32, 63] {
        let board = BoardState::new(1234, num_bombs, Rules::default());
        assert_eq!(bombs(&board), u32::from(num_bombs), "num_bombs={num_bombs}");
        assert_eq!(board.num_bombs, num_bombs);
    }
}

#[test]
fn bomb_count_is_clamped_to_63() {
    let board = BoardState::new(9, 200, Rules::default());
    assert_eq!(board.num_bombs, 63);
    assert_eq!(bombs(&board), 63);
}

#[test]
fn adjacency_counts_match_recomputation() {
    for seed in 0..50u64 {
        let board = BoardState::new(seed, 12, Rules::default());
        for cell in 0..64u8 {
            assert_eq!(
                board.adj_counts[cell as usize],
                expected_adj(&board, cell),
                "seed={seed} cell={cell}"
            );
        }
    }
}

#[test]
fn fresh_board_is_pristine() {
    let board = BoardState::new(7, 10, Rules::default());
    assert_eq!(board.revealed_mask, 0);
    assert_eq!(board.flagged_mask, 0);
    assert_eq!(board.detonated_mask, 0);
    assert_eq!(board.move_count, 0);
    assert_eq!(board.strikes, 0);
    assert!(!board.is_terminal);
    assert!(!board.is_won);
}

// -----------------------------------------------------------------------------
// Safe-first-click rule
// -----------------------------------------------------------------------------

#[test]
fn first_reveal_is_always_safe() {
    for seed in 0..128u64 {
        for cell in 0..64u8 {
            let mut board = BoardState::new(seed, 20, Rules::default());
            let result = board.step(reveal(cell));
            assert!(result.valid_move, "seed={seed} cell={cell}");
            assert!(!result.hit_bomb, "seed={seed} cell={cell}");
            assert_eq!(board.strikes, 0);
        }
    }
}

#[test]
fn first_reveal_relocates_an_underlying_bomb() {
    let mut board = BoardState::new(555, 20, Rules::default());
    let target = first_bomb(&board);
    let before = board.mine_mask;

    let result = board.step(reveal(target));

    assert!(!result.hit_bomb);
    assert_eq!(bombs(&board), 20, "bomb count must be preserved");
    assert_eq!(
        board.mine_mask & bit(target),
        0,
        "bomb left the clicked cell"
    );
    assert_ne!(board.mine_mask, before, "layout should have changed");
    assert!(board.revealed_mask & bit(target) != 0);
}

#[test]
fn safe_first_click_does_not_relocate_when_cell_is_safe() {
    let mut board = BoardState::new(555, 20, Rules::default());
    // Ensure the chosen cell is genuinely safe before the move.
    let safe = first_safe(&board);
    let before = board.mine_mask;
    board.step(reveal(safe));
    assert_eq!(board.mine_mask, before);
}

#[test]
fn safe_first_reveal_still_applies_after_a_flag() {
    // Flagging is not a reveal: every single-bomb board must still relocate
    // the bomb when the first reveal targets it.
    for bomb in 0..64u8 {
        let mut board = BoardState::from_mine_mask(bit(bomb), Rules::default());
        let flag_cell = (bomb + 1) % 64;

        let flagged = board.step(flag(flag_cell));
        assert!(flagged.valid_move, "flag on bomb {bomb} should be valid");

        let result = board.step(reveal(bomb));
        assert!(result.valid_move);
        assert!(
            !result.hit_bomb,
            "first reveal detonated after a flag on bomb {bomb}"
        );
        assert_eq!(board.strikes, 0);
        assert_eq!(
            board.mine_mask.count_ones(),
            1,
            "bomb count must be preserved on bomb {bomb}"
        );
        assert_eq!(
            board.mine_mask & bit(bomb),
            0,
            "bomb left the clicked cell on bomb {bomb}"
        );
    }
}

// -----------------------------------------------------------------------------
// Flagging
// -----------------------------------------------------------------------------

#[test]
fn flag_toggles_on_and_off() {
    let mut board = BoardState::new(3, 10, Rules::default());

    let on = board.step(flag(0));
    assert!(on.valid_move);
    assert_eq!(board.flagged_mask & bit(0), bit(0));
    assert_eq!(board.move_count, 1);

    let off = board.step(flag(0));
    assert!(off.valid_move);
    assert_eq!(board.flagged_mask & bit(0), 0);
    assert_eq!(board.move_count, 2);
}

#[test]
fn cannot_flag_a_revealed_cell() {
    let mut board = BoardState::new(3, 10, Rules::default());
    let safe = first_safe(&board);
    board.step(reveal(safe));
    assert!(board.revealed_mask & bit(safe) != 0);

    let result = board.step(flag(safe));
    assert!(!result.valid_move);
}

#[test]
fn cannot_reveal_a_flagged_cell() {
    let mut board = BoardState::new(3, 10, Rules::default());
    let safe = first_safe(&board);
    board.step(flag(safe));

    let result = board.step(reveal(safe));
    assert!(!result.valid_move);
    assert_eq!(board.revealed_mask & bit(safe), 0);
}

#[test]
fn cascade_does_not_reveal_flagged_cells() {
    // Flag an unrevealed cell, then trigger a reveal elsewhere and confirm the
    // flag survives and the cell stays hidden.
    let mut board = BoardState::new(77, 10, Rules::default());
    let safe = first_safe(&board);

    // Pick a different, unrevealed cell to flag.
    let flagged = (0..64u8)
        .find(|&i| i != safe && (board.mine_mask & bit(i)) == 0)
        .unwrap();
    board.step(flag(flagged));
    assert!(board.flagged_mask & bit(flagged) != 0);

    board.step(reveal(safe));

    assert!(board.flagged_mask & bit(flagged) != 0, "flag must persist");
    assert_eq!(
        board.revealed_mask & bit(flagged),
        0,
        "flagged stays hidden"
    );
}

// -----------------------------------------------------------------------------
// Cascade / flood fill
// -----------------------------------------------------------------------------

#[test]
fn zero_cell_reveal_cascades_to_neighbors() {
    let mut board = BoardState::new(321, 5, Rules::default());
    // Find a zero-adjacency safe cell to guarantee a cascade.
    let zero =
        (0..64u8).find(|&i| (board.mine_mask & bit(i)) == 0 && board.adj_counts[i as usize] == 0);

    if let Some(zero) = zero {
        let result = board.step(reveal(zero));
        // A zero cell plus its (at least one) neighbors must all be revealed.
        let revealed = board.revealed_mask & !board.mine_mask;
        assert!(result.newly_revealed >= 2);
        assert_eq!(revealed.count_ones(), u32::from(result.newly_revealed));
    }
}

#[test]
fn reveal_never_exposes_a_mine_on_a_safe_move() {
    let mut board = BoardState::new(88, 15, Rules::default());
    let safe = first_safe(&board);
    let result = board.step(reveal(safe));
    assert!(!result.hit_bomb);
    assert_eq!(board.revealed_mask & board.mine_mask, 0);
}

// -----------------------------------------------------------------------------
// Strikes, terminal states, and rules
// -----------------------------------------------------------------------------

#[test]
fn hitting_a_bomb_strikes_and_ends_classic_game() {
    let mut board = BoardState::new(7, 20, Rules { max_strikes: 1 });
    let safe = first_safe(&board);
    board.step(reveal(safe));
    assert!(!board.is_terminal);

    let mine = first_bomb(&board);
    let result = board.step(reveal(mine));

    assert!(result.hit_bomb);
    assert!(result.terminal);
    assert!(!result.won);
    assert!(board.is_terminal);
    assert!(!board.is_won);
    assert_eq!(board.strikes, 1);
    assert!(board.detonated_mask & bit(mine) != 0);
}

#[test]
fn shielded_rules_allow_multiple_strikes() {
    let mut board = BoardState::new(7, 20, Rules { max_strikes: 3 });
    let safe = first_safe(&board);
    board.step(reveal(safe));

    let mines = all_bombs(&board);
    assert!(mines.len() >= 3);

    let r1 = board.step(reveal(mines[0]));
    assert!(r1.hit_bomb);
    assert!(!r1.terminal, "survives first strike");
    assert_eq!(board.strikes, 1);

    let r2 = board.step(reveal(mines[1]));
    assert!(r2.hit_bomb);
    assert!(!r2.terminal, "survives second strike");
    assert_eq!(board.strikes, 2);

    let r3 = board.step(reveal(mines[2]));
    assert!(r3.hit_bomb);
    assert!(r3.terminal, "third strike ends the game");
    assert_eq!(board.strikes, 3);
    assert!(board.is_terminal);
    assert!(!board.is_won);
}

#[test]
fn cannot_reveal_an_already_revealed_cell() {
    let mut board = BoardState::new(4, 10, Rules::default());
    let safe = first_safe(&board);
    board.step(reveal(safe));
    let result = board.step(reveal(safe));
    assert!(!result.valid_move);
}

#[test]
fn moves_after_terminal_are_invalid() {
    let mut board = BoardState::new(7, 20, Rules { max_strikes: 1 });
    let safe = first_safe(&board);
    board.step(reveal(safe));
    board.step(reveal(first_bomb(&board)));
    assert!(board.is_terminal);

    let result = board.step(reveal(0));
    assert!(!result.valid_move);
}

#[test]
fn out_of_range_cell_is_invalid() {
    let mut board = BoardState::new(1, 10, Rules::default());
    let result = board.step(Action {
        action_type: ActionType::Reveal,
        cell: 64,
    });
    assert!(!result.valid_move);
    assert_eq!(board.move_count, 0);
}

#[test]
fn zero_bombs_is_won_on_first_reveal() {
    let mut board = BoardState::new(5, 0, Rules::default());
    let result = board.step(reveal(0));
    assert!(result.valid_move);
    assert!(result.won);
    assert!(result.terminal);
    assert!(board.is_won);
    assert!(board.is_terminal);
    // Every cell is safe and should have cascaded open.
    assert_eq!(board.revealed_mask, u64::MAX);
}

// -----------------------------------------------------------------------------
// Observation encoding
// -----------------------------------------------------------------------------

#[test]
fn observation_encodes_hidden_flagged_detonated_and_counts() {
    let mut board = BoardState::new(11, 20, Rules::default());
    let mut obs = [0f32; 64];

    board.get_observation(&mut obs);
    assert!(obs.iter().all(|&v| v == -1.0), "fresh board is all hidden");

    // Flagged cell -> -0.5
    board.step(flag(0));
    board.get_observation(&mut obs);
    assert_eq!(obs[0], -0.5);

    // Revealed numbered cell -> count / 10.0
    let numbered = first_safe_with_count(&board, 1);
    board.step(reveal(numbered));
    board.get_observation(&mut obs);
    let expected = f32::from(board.adj_counts[numbered as usize]) / 10.0;
    assert!((obs[numbered as usize] - expected).abs() < 1e-6);

    // Detonated bomb -> -2.0
    let mine = first_bomb(&board);
    board.step(reveal(mine));
    board.get_observation(&mut obs);
    assert_eq!(obs[mine as usize], -2.0);
}

#[test]
fn observation_range_is_within_documented_bounds() {
    let mut board = BoardState::new(2, 20, Rules::default());
    board.step(flag(3));
    let mut obs = [0f32; 64];
    board.get_observation(&mut obs);
    for (i, &v) in obs.iter().enumerate() {
        assert!(
            (-2.0..=0.8).contains(&v),
            "obs[{i}] = {v} out of documented range"
        );
    }
}

// -----------------------------------------------------------------------------
// Action mask
// -----------------------------------------------------------------------------

#[test]
fn action_mask_is_all_ones_on_fresh_board() {
    let board = BoardState::new(6, 10, Rules::default());
    let mut mask = [0f32; 64];
    board.get_action_mask(&mut mask);
    assert!(mask.iter().all(|&m| m == 1.0));
}

#[test]
fn action_mask_clears_flagged_and_revealed_cells() {
    let mut board = BoardState::new(6, 10, Rules::default());

    board.step(flag(3));
    let safe = first_safe(&board);
    board.step(reveal(safe));

    let mut mask = [0f32; 64];
    board.get_action_mask(&mut mask);

    assert_eq!(mask[3], 0.0, "flagged cell masked out");
    assert_eq!(mask[safe as usize], 0.0, "revealed cell masked out");

    // Every masked-out cell must genuinely be revealed or flagged.
    for i in 0..64u8 {
        let is_open_or_flagged = (board.revealed_mask | board.flagged_mask) & bit(i) != 0;
        if is_open_or_flagged {
            assert_eq!(mask[i as usize], 0.0, "cell {i}");
        }
    }
}

#[test]
fn action_mask_is_all_zero_after_terminal_loss() {
    // The first reveal is safe, so open a safe cell before stepping on a mine.
    let mut board = BoardState::from_mine_mask(bit(0) | bit(1), Rules::default());
    let safe = board.step(reveal(0));
    assert!(safe.valid_move && !safe.hit_bomb);

    let result = board.step(reveal(1));
    assert!(result.terminal && !result.won);

    let mut mask = [0.5f32; 64];
    board.get_action_mask(&mut mask);
    assert!(
        mask.iter().all(|&m| m == 0.0),
        "a terminal board must expose no legal actions"
    );
}

// -----------------------------------------------------------------------------
// Deterministic layout constructor
// -----------------------------------------------------------------------------

#[test]
fn from_mine_mask_pins_exact_layout() {
    // Bombs at (row 0, col 0) = cell 0, and cell 27; nothing else.
    let mask = bit(0) | bit(27);
    let board = BoardState::from_mine_mask(mask, Rules::default());

    assert_eq!(board.mine_mask, mask);
    assert_eq!(board.num_bombs, 2);
    assert_eq!(board.seed, 0);
    assert_eq!(board.revealed_mask, 0);
    assert_eq!(board.flagged_mask, 0);
    assert_eq!(board.detonated_mask, 0);
    assert_eq!(board.move_count, 0);
    assert!(!board.is_terminal && !board.is_won);

    // Adjacency must be recomputed from the pinned mask.
    for cell in 0..64u8 {
        assert_eq!(
            board.adj_counts[cell as usize],
            expected_adj(&board, cell),
            "cell {cell}"
        );
    }
}

#[test]
fn from_mine_mask_zero_is_an_empty_board() {
    let board = BoardState::from_mine_mask(0, Rules::default());
    assert_eq!(board.mine_mask, 0);
    assert_eq!(board.num_bombs, 0);
    assert!(board.adj_counts.iter().all(|&c| c == 0));
}

#[test]
fn from_mine_mask_full_board_has_no_safe_cells() {
    let mut board = BoardState::from_mine_mask(u64::MAX, Rules::default());
    assert_eq!(board.num_bombs, 64);
    assert_eq!(board.mine_mask, u64::MAX);

    // No free cell exists for safe-first-click relocation, so the first
    // reveal detonates and the board can never be won.
    let result = board.step(reveal(0));
    assert!(result.valid_move);
    assert!(result.hit_bomb);
    assert!(result.terminal);
    assert!(!result.won);
    assert!(!board.is_won);
    assert_eq!(
        board.mine_mask,
        u64::MAX,
        "a full board must keep every mine"
    );

    let mut mask = [1f32; 64];
    board.get_action_mask(&mut mask);
    assert!(mask.iter().all(|&m| m == 0.0));
}

#[test]
fn from_mine_mask_is_playable() {
    // A pinned layout behaves like any other board: reveal away from the bomb.
    let mut board = BoardState::from_mine_mask(bit(27), Rules::default());
    let result = board.step(reveal(0));
    assert!(result.valid_move);
    assert!(!result.hit_bomb);
    assert!(result.newly_revealed >= 1);
}
