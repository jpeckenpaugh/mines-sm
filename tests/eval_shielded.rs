//! Sampled eval coverage for **shielded rules** (`max_strikes = 3`).
//!
//! Shielded termination needs at least 3 distinct bombs, so this cannot reuse
//! the 1- and 2-bomb grids. Enumerating `C(64,3) = 41664` is cheap, but we only
//! need a sample: we take a **1% stride** (every 100th layout) of the minimal
//! 3-bomb family, and add a small deterministic PRNG sample of 4..=8-bomb
//! layouts to exercise the case where bombs remain after the game ends.
//!
//! For each sampled layout we verify the strike ladder under shielded rules:
//! strike 1 and 2 survive, and strike 3 terminates in a loss.

mod common;

use common::{bit, random_mask, reveal, SplitMix64};
use mine_sm::{BoardState, Rules, StepResult};

const SHIELDED: Rules = Rules { max_strikes: 3 };

/// The sampled set of shielded layouts.
fn sampled_layouts() -> Vec<u64> {
    let mut masks = Vec::new();

    // 1% stride over the 3-bomb family: C(64,3) = 41664 -> indices 0,100,...
    let mut index: u32 = 0;
    for a in 0..64u8 {
        for b in (a + 1)..64u8 {
            for c in (b + 1)..64u8 {
                if index.is_multiple_of(100) {
                    masks.push(bit(a) | bit(b) | bit(c));
                }
                index += 1;
            }
        }
    }
    assert_eq!(index, 41_664, "C(64,3)");

    // A handful of 4..=8-bomb layouts (bombs remain after the game ends).
    for k in 4..=8u8 {
        let mut rng = SplitMix64(0xA11CE ^ u64::from(k));
        for _ in 0..30 {
            masks.push(random_mask(&mut rng, k));
        }
    }

    masks
}

/// Returns the first three distinct bomb cells of a layout.
fn three_distinct_bombs(mines: u64) -> (u8, u8, u8) {
    let mut it = (0..64u8).filter(|&c| mines & bit(c) != 0);
    let a = it.next().expect("bomb 1");
    let b = it.next().expect("bomb 2");
    let c = it.next().expect("bomb 3");
    (a, b, c)
}

/// Safe opening move that cannot cascade (numbered cell), then three distinct
/// bomb detonations.
fn shielded_strike_line(mines: u64) -> (BoardState, Vec<StepResult>) {
    let probe = BoardState::from_mine_mask(mines, SHIELDED);
    let safe = (0..64u8)
        .find(|&c| (mines & bit(c)) == 0 && probe.adj_counts[c as usize] > 0)
        .expect("any board with bombs has a numbered neighbour");
    let (a, b, c) = three_distinct_bombs(mines);

    let mut board = BoardState::from_mine_mask(mines, SHIELDED);
    let results = vec![
        board.step(reveal(safe)),
        board.step(reveal(a)),
        board.step(reveal(b)),
        board.step(reveal(c)),
    ];
    (board, results)
}

// -----------------------------------------------------------------------------
// Sampling sanity
// -----------------------------------------------------------------------------

#[test]
fn sample_is_about_one_percent_of_the_three_bomb_family() {
    let masks = sampled_layouts();
    let three_bomb = masks.iter().filter(|m| m.count_ones() == 3).count();
    let higher = masks.iter().filter(|m| m.count_ones() > 3).count();

    assert_eq!(three_bomb, 417, "1% stride of C(64,3)");
    assert_eq!(higher, 150, "30 layouts each for k = 4..=8");
    assert!(masks.iter().all(|m| m.count_ones() >= 3));
}

// -----------------------------------------------------------------------------
// The shielded strike ladder
// -----------------------------------------------------------------------------

#[test]
fn shielded_strike_ladder_survives_twice_then_loses() {
    for mines in sampled_layouts() {
        let k = mines.count_ones();
        let (board, results) = shielded_strike_line(mines);
        assert_eq!(results.len(), 4, "k={k}: safe + three strikes");

        // Move 0: safe numbered reveal, no strike, game continues.
        assert!(results[0].valid_move && !results[0].hit_bomb, "k={k}");
        assert!(!results[0].terminal, "k={k}");

        // Strikes 1 and 2 detonate but do NOT terminate.
        assert!(results[1].hit_bomb && !results[1].terminal, "k={k}");
        assert!(results[2].hit_bomb && !results[2].terminal, "k={k}");

        // Strike 3 detonates and terminates as a loss.
        assert!(results[3].hit_bomb, "k={k}");
        assert!(results[3].terminal && !results[3].won, "k={k}");

        assert!(board.is_terminal && !board.is_won, "k={k}");
        assert_eq!(board.strikes, 3, "k={k}");
    }
}

#[test]
fn shielded_terminal_state_matches_strike_count() {
    for mines in sampled_layouts() {
        let k = mines.count_ones();
        let (board, _) = shielded_strike_line(mines);
        // Game ended the moment the 3rd strike landed.
        assert_eq!(board.strikes, u32::from(SHIELDED.max_strikes) as u8);
        assert!(board.is_terminal);

        // The three hit cells are marked detonated and revealed.
        assert_eq!(board.detonated_mask.count_ones(), 3, "k={k}");
        assert_eq!(
            board.detonated_mask & !board.revealed_mask,
            0,
            "k={k}: detonated cells are revealed"
        );
        // Remaining bombs survive untouched on the board.
        assert_eq!(
            (board.mine_mask & !board.detonated_mask).count_ones(),
            k - 3,
            "k={k}: bombs left after terminal"
        );
    }
}

#[test]
fn shielded_rules_continue_where_classic_would_end() {
    // Same board and opening; classic rules terminate on strike 1, shielded
    // stays alive. This pins the rule parameter's effect.
    let mines = bit(0) | bit(27) | bit(40);
    let probe = BoardState::from_mine_mask(mines, SHIELDED);
    let safe = (0..64u8)
        .find(|&c| (mines & bit(c)) == 0 && probe.adj_counts[c as usize] > 0)
        .unwrap();
    let (a, _, _) = three_distinct_bombs(mines);

    let mut classic = BoardState::from_mine_mask(mines, Rules::default());
    classic.step(reveal(safe));
    let classic_strike = classic.step(reveal(a));
    assert!(classic_strike.hit_bomb && classic_strike.terminal);
    assert_eq!(classic.strikes, 1);

    let mut shielded = BoardState::from_mine_mask(mines, SHIELDED);
    shielded.step(reveal(safe));
    let shielded_strike = shielded.step(reveal(a));
    assert!(shielded_strike.hit_bomb && !shielded_strike.terminal);
    assert_eq!(shielded.strikes, 1);
}

#[test]
fn shielded_boards_can_still_be_won() {
    // Winning is independent of the strike rule: reveal all safe cells.
    for mines in sampled_layouts() {
        let mut board = BoardState::from_mine_mask(mines, SHIELDED);
        let mut saw_terminal = false;
        while !board.is_terminal {
            let next = (0..64u8).find(|&c| {
                let b = bit(c);
                (mines & b) == 0 && (board.revealed_mask & b) == 0 && (board.flagged_mask & b) == 0
            });
            let Some(cell) = next else { break };
            let r = board.step(reveal(cell));
            saw_terminal |= r.terminal;
        }
        assert!(board.is_won && board.is_terminal, "shielded win");
        assert_eq!(board.strikes, 0);
        assert_eq!(board.revealed_mask, !mines);
        assert!(saw_terminal);
    }
}

#[test]
fn moves_after_shielded_terminal_are_invalid() {
    let mines = bit(5) | bit(30) | bit(61) | bit(12);
    let (mut board, _) = shielded_strike_line(mines);
    assert!(board.is_terminal);

    let next_hidden = (0..64u8)
        .find(|&c| (board.revealed_mask & bit(c)) == 0)
        .unwrap();
    let after = board.step(reveal(next_hidden));
    assert!(!after.valid_move);
}
