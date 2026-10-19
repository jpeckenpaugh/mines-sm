//! Cross-language goldens shared with the browser harness.
//!
//! `engine.js` validates its own port against these same values in
//! `scripts/check-js.js`. Pinning them here too means a change to either
//! implementation fails a test instead of drifting silently.
//!
//! Keep this table in sync with `GOLDENS` in `engine.js`.

mod common;

use common::{bit, win_trace};
use mine_sm::{BoardState, Rules};

#[test]
fn seeded_layouts_match_the_browser_goldens() {
    let cases: [(u64, u8, u64); 5] = [
        (42, 10, 0x8004_4010_2034_0048),
        (6, 1, 0x0000_0000_0000_0001),
        (1, 5, 0x0200_0080_4000_0802),
        (7, 3, 0x0000_0000_1080_0004),
        (0, 0, 0x0000_0000_0000_0000),
    ];

    for (seed, bombs, expected) in cases {
        let board = BoardState::new(seed, bombs, Rules::default());
        assert_eq!(
            board.mine_mask, expected,
            "seed {seed} / {bombs} bombs drifted from the browser golden"
        );
    }
}

#[test]
fn win_traces_match_the_browser_goldens() {
    let cases: [(&[u8], &[u32]); 11] = [
        (&[0], &[1, 62]),
        (&[1], &[1, 1, 61]),
        (&[3], &[63]),
        (&[27], &[63]),
        (&[0, 9], &[1, 1, 59, 1]),
        (&[0, 1], &[1, 61]),
        (&[0, 63], &[1, 61]),
        (&[0, 1, 8, 9], &[1, 59]),
        (&[0, 9, 18, 27], &[1, 1, 57, 1]),
        (&[18, 19, 20, 26, 28], &[58, 1]),
        (&[18, 19, 20, 26, 28, 34, 35, 36], &[55, 1]),
    ];

    for (cells, expected) in cases {
        let mines = cells.iter().fold(0u64, |mask, &cell| mask | bit(cell));
        assert_eq!(
            win_trace(mines),
            expected,
            "win trace for cells {cells:?} drifted from the browser golden"
        );
    }
}
