//! Property-based invariant tests for the `mine-sm` state machine.
//!
//! These complement the deterministic unit/eval suites by exercising random
//! boards and action sequences. Every property is bounded and reproducible:
//! `proptest` records the seed for any failure.
//!
//! Run with `cargo test --test proptest`.

use mine_sm::{Action, ActionType, BoardState, Rules};
use proptest::prelude::*;

fn bit(cell: u8) -> u64 {
    1u64 << cell
}

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

fn any_action() -> impl Strategy<Value = Action> {
    (any::<bool>(), 0u8..64).prop_map(|(is_reveal, cell)| Action {
        action_type: if is_reveal {
            ActionType::Reveal
        } else {
            ActionType::Flag
        },
        cell,
    })
}

fn any_rules() -> impl Strategy<Value = Rules> {
    (1u8..=4).prop_map(|max_strikes| Rules { max_strikes })
}

fn safe_revealed(board: &BoardState) -> u32 {
    (board.revealed_mask & !board.mine_mask).count_ones()
}

fn total_safe(board: &BoardState) -> u32 {
    64 - board.mine_mask.count_ones()
}

fn play(board: &mut BoardState, actions: &[Action]) {
    for action in actions {
        if board.is_terminal {
            break;
        }
        board.step(*action);
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        .. ProptestConfig::default()
    })]

    /// Observable bitboards stay internally consistent through arbitrary play.
    #[test]
    fn bitboards_stay_consistent(
        seed in any::<u64>(),
        bombs in 0u8..=63,
        rules in any_rules(),
        actions in prop::collection::vec(any_action(), 0..64),
    ) {
        let mut board = BoardState::new(seed, bombs, rules);
        play(&mut board, &actions);

        prop_assert_eq!(
            board.revealed_mask & board.flagged_mask,
            0,
            "a cell cannot be both revealed and flagged"
        );
        prop_assert_eq!(
            board.detonated_mask & !board.revealed_mask,
            0,
            "detonated cells must be revealed"
        );
        prop_assert_eq!(
            board.detonated_mask & !board.mine_mask,
            0,
            "only mines can detonate"
        );
        prop_assert_eq!(
            board.revealed_mask & board.mine_mask,
            board.detonated_mask,
            "revealed mines are exactly the detonated mines"
        );
        prop_assert_eq!(
            board.mine_mask.count_ones(),
            board.num_bombs as u32,
            "relocation must preserve the bomb count"
        );
        prop_assert!(board.strikes <= board.rules.max_strikes);
    }

    /// Invalid and post-terminal moves are complete no-ops.
    #[test]
    fn invalid_and_terminal_moves_are_noops(
        seed in any::<u64>(),
        bombs in 0u8..=63,
        rules in any_rules(),
        actions in prop::collection::vec(any_action(), 0..64),
        probes in prop::collection::vec(any_action(), 0..32),
    ) {
        let mut board = BoardState::new(seed, bombs, rules);
        for action in &actions {
            let before = board.clone();
            let result = board.step(*action);
            if !result.valid_move {
                prop_assert_eq!(&board, &before, "invalid move mutated the board");
            }
        }

        if board.is_terminal {
            let before = board.clone();
            for action in &probes {
                let result = board.step(*action);
                prop_assert!(!result.valid_move);
                prop_assert_eq!(&board, &before, "post-terminal move mutated the board");
            }
        }
    }

    /// The action mask exactly matches reveal validity, and is all-zero when
    /// the game is over.
    #[test]
    fn action_mask_matches_step(
        seed in any::<u64>(),
        bombs in 0u8..=63,
        rules in any_rules(),
        prelude in prop::collection::vec(any_action(), 0..32),
    ) {
        let mut board = BoardState::new(seed, bombs, rules);
        play(&mut board, &prelude);

        let mut mask = [0.5f32; 64];
        board.get_action_mask(&mut mask);

        if board.is_terminal {
            prop_assert!(mask.iter().all(|&m| m == 0.0));
        } else {
            for cell in 0..64u8 {
                let mut probe = board.clone();
                let result = probe.step(reveal(cell));
                prop_assert_eq!(
                    mask[cell as usize] == 1.0,
                    result.valid_move,
                    "mask/step disagreement at cell {}",
                    cell
                );
                prop_assert!(mask[cell as usize] == 0.0 || mask[cell as usize] == 1.0);
            }
        }
    }

    /// Observation values stay in the documented set and match the bitboards.
    #[test]
    fn observation_matches_state(
        seed in any::<u64>(),
        bombs in 0u8..=63,
        rules in any_rules(),
        actions in prop::collection::vec(any_action(), 0..48),
    ) {
        let mut board = BoardState::new(seed, bombs, rules);
        play(&mut board, &actions);

        let mut obs = [0f32; 64];
        board.get_observation(&mut obs);

        for cell in 0..64u8 {
            let i = cell as usize;
            let value = obs[i];
            prop_assert!(
                (-2.0..=0.8).contains(&value),
                "observation {} out of range: {}",
                i,
                value
            );

            if board.detonated_mask & bit(cell) != 0 {
                prop_assert_eq!(value, -2.0);
            } else if board.flagged_mask & bit(cell) != 0 {
                prop_assert_eq!(value, -0.5);
            } else if board.revealed_mask & bit(cell) == 0 {
                prop_assert_eq!(value, -1.0);
            } else {
                prop_assert_eq!(value, board.adj_counts[i] as f32 / 10.0);
            }
        }

        // Adjacency counts always match an independent recomputation.
        for cell in 0..64u8 {
            let (neighbors, count) = neighbors(cell);
            let expected = neighbors[..count as usize]
                .iter()
                .filter(|&&n| board.mine_mask & bit(n) != 0)
                .count() as u8;
            prop_assert_eq!(board.adj_counts[cell as usize], expected);
        }
    }

    /// Seeded construction is deterministic and `from_mine_mask` round-trips.
    #[test]
    fn determinism_and_roundtrip(
        seed in any::<u64>(),
        bombs in 0u8..=63,
        rules in any_rules(),
        actions in prop::collection::vec(any_action(), 0..48),
    ) {
        let mut first = BoardState::new(seed, bombs, rules);
        let mut second = BoardState::new(seed, bombs, rules);
        prop_assert_eq!(&first, &second);

        play(&mut first, &actions);
        play(&mut second, &actions);
        prop_assert_eq!(&first, &second);

        let pinned = BoardState::from_mine_mask(first.mine_mask, rules);
        prop_assert_eq!(pinned.mine_mask, first.mine_mask);
        prop_assert_eq!(pinned.adj_counts, first.adj_counts);
        prop_assert_eq!(pinned.num_bombs, first.mine_mask.count_ones() as u8);
    }

    /// The first reveal is safe even after arbitrary flagging.
    #[test]
    fn safe_first_reveal_survives_flags(
        mask in any::<u64>(),
        rules in any_rules(),
        flagged in prop::collection::vec(0u8..64, 0..16),
    ) {
        // A full-mine fixture has no free cell to relocate into; it is the
        // documented exception.
        prop_assume!(mask != u64::MAX);

        let mut board = BoardState::from_mine_mask(mask, rules);
        for cell in flagged {
            board.step(flag(cell));
        }

        let target = (0..64u8).find(|&cell| {
            board.mine_mask & bit(cell) != 0 && board.flagged_mask & bit(cell) == 0
        });

        if let Some(target) = target {
            let bombs_before = board.mine_mask.count_ones();
            let result = board.step(reveal(target));
            prop_assert!(result.valid_move);
            prop_assert!(!result.hit_bomb, "first reveal detonated after flags");
            prop_assert_eq!(board.strikes, 0);
            prop_assert_eq!(board.mine_mask.count_ones(), bombs_before);
        }
    }

    /// Zero-cell cascades are closed: every reachable unflagged safe neighbor
    /// of an opened zero cell is also open.
    #[test]
    fn cascade_is_closed(
        mask in any::<u64>(),
        rules in any_rules(),
        actions in prop::collection::vec(any_action(), 0..48),
    ) {
        prop_assume!(mask != u64::MAX);
        let mut board = BoardState::from_mine_mask(mask, rules);
        play(&mut board, &actions);

        for cell in 0..64u8 {
            let opened_zero = board.revealed_mask & bit(cell) != 0
                && board.mine_mask & bit(cell) == 0
                && board.adj_counts[cell as usize] == 0;
            if !opened_zero {
                continue;
            }

            let (neighbors, count) = neighbors(cell);
            for &neighbor in &neighbors[..count as usize] {
                let n_bit = bit(neighbor);
                let blocked = board.flagged_mask & n_bit != 0 || board.mine_mask & n_bit != 0;
                if !blocked {
                    prop_assert!(
                        board.revealed_mask & n_bit != 0,
                        "cascade left neighbor {} of zero cell {} closed",
                        neighbor,
                        cell
                    );
                }
            }
        }
    }

    /// Terminal outcomes agree with the rules.
    #[test]
    fn win_loss_consistency(
        seed in any::<u64>(),
        bombs in 0u8..=63,
        rules in any_rules(),
        actions in prop::collection::vec(any_action(), 0..64),
    ) {
        let mut board = BoardState::new(seed, bombs, rules);
        play(&mut board, &actions);

        prop_assert!(safe_revealed(&board) <= total_safe(&board));

        if board.is_won {
            prop_assert!(board.revealed_mask & !board.mine_mask == !board.mine_mask);
            prop_assert_eq!(safe_revealed(&board), total_safe(&board));
            prop_assert_eq!(board.detonated_mask, 0);
        } else if board.is_terminal {
            prop_assert!(board.strikes >= board.rules.max_strikes);
            prop_assert!(!board.is_won);
        }
    }
}

fn neighbors(cell: u8) -> ([u8; 8], u8) {
    let mut out = [0u8; 8];
    let mut count = 0u8;
    let row = (cell / 8) as i8;
    let col = (cell % 8) as i8;

    for dr in -1..=1 {
        for dc in -1..=1 {
            if dr == 0 && dc == 0 {
                continue;
            }
            let nr = row + dr;
            let nc = col + dc;
            if (0..8).contains(&nr) && (0..8).contains(&nc) {
                out[count as usize] = (nr * 8 + nc) as u8;
                count += 1;
            }
        }
    }

    (out, count)
}
