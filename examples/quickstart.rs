//! Quickstart: drive a game by hand and inspect the agent-facing tensors.
//!
//! Run with:
//!     cargo run --example quickstart
//!
//! This example only relies on the crate's public API and uses `std` (the
//! library itself is `no_std`).

use mine_sm::{Action, ActionType, BoardState, Rules};

/// Render the board from the *observable* layer only (what an agent would see).
fn render(board: &BoardState) -> String {
    let mut obs = [0f32; 64];
    board.get_observation(&mut obs);

    let mut out = String::new();
    for row in 0..8 {
        for col in 0..8 {
            let v = obs[row * 8 + col];
            if (v + 2.0).abs() < f32::EPSILON {
                out.push('*'); // detonated bomb
            } else if (v + 0.5).abs() < f32::EPSILON {
                out.push('F'); // flagged
            } else if (v + 1.0).abs() < f32::EPSILON {
                out.push('.'); // hidden
            } else {
                // Revealed adjacency count, 0..8.
                match (v * 10.0).round() as u8 {
                    0 => out.push(' '),
                    n => out.push(char::from_digit(u32::from(n), 10).unwrap()),
                }
            }
            out.push(' ');
        }
        out.push('\n');
    }
    out
}

fn main() {
    // Deterministic board: seed 42, 10 bombs, classic sudden-death rules.
    let mut board = BoardState::new(42, 10, Rules::default());

    println!("== initial (agent view) ==");
    print!("{}", render(&board));

    // 1) Open a cell. The very first reveal is guaranteed safe.
    let first = Action {
        action_type: ActionType::Reveal,
        cell: 27,
    };
    let result = board.step(first);
    println!(
        "\nreveal 27 -> valid={} newly_revealed={} hit_bomb={}",
        result.valid_move, result.newly_revealed, result.hit_bomb
    );

    // 2) Flag a cell, then try to reveal it (should be rejected).
    board.step(Action {
        action_type: ActionType::Flag,
        cell: 0,
    });
    let rejected = board.step(Action {
        action_type: ActionType::Reveal,
        cell: 0,
    });
    println!("reveal flagged cell 0 -> valid={}", rejected.valid_move);

    println!("\n== after moves ==");
    print!("{}", render(&board));

    // 3) Inspect the action mask (1.0 = legal reveal target).
    let mut mask = [0f32; 64];
    board.get_action_mask(&mut mask);
    let legal = mask.iter().filter(|&&m| m == 1.0).count();
    println!("\nlegal reveal targets: {legal} / 64");
    println!(
        "terminal={} won={} strikes={}",
        board.is_terminal, board.is_won, board.strikes
    );

    // 4) Play out greedily by always revealing the first legal cell until the
    //    game ends (this will usually lose; it just demonstrates the loop).
    while !board.is_terminal {
        board.get_action_mask(&mut mask);
        let Some(cell) = mask.iter().position(|&m| m == 1.0) else {
            break;
        };
        let r = board.step(Action {
            action_type: ActionType::Reveal,
            cell: cell as u8,
        });
        if r.terminal {
            println!(
                "\ngame over: won={} strikes={} moves={}",
                r.won, board.strikes, board.move_count
            );
        }
    }
}
