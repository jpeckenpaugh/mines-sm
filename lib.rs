#![no_std]

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionType {
    Reveal,
    Flag,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Action {
    pub action_type: ActionType,
    pub cell: u8, // 0..63
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rules {
    pub max_strikes: u8, // 1 for classic sudden death, 3 for shielded, etc.
}

impl Default for Rules {
    fn default() -> Self {
        Self { max_strikes: 1 }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardState {
    pub seed: u64,
    pub num_bombs: u8,
    pub move_count: u16,
    pub strikes: u8,
    pub rules: Rules,

    // Internal ground truth (Hidden from agent)
    pub mine_mask: u64,
    pub adj_counts: [u8; 64],

    // Player/Model observable layer
    pub revealed_mask: u64,
    pub flagged_mask: u64,
    pub detonated_mask: u64,

    // Status flags
    pub is_terminal: bool,
    pub is_won: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepResult {
    pub valid_move: bool,
    pub newly_revealed: u8,
    pub hit_bomb: bool,
    pub terminal: bool,
    pub won: bool,
}

// -----------------------------------------------------------------------------
// Deterministic SplitMix64 PRNG (Byte-for-byte identical on Wasm & Native x86/ARM)
// -----------------------------------------------------------------------------
struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    #[inline]
    fn new(seed: u64) -> Self {
        // Guard against zero state if necessary
        Self { state: seed }
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    #[inline]
    fn next_bounded(&mut self, bound: u64) -> u64 {
        self.next_u64() % bound
    }
}

// -----------------------------------------------------------------------------
// Bitboard Coordinate Utilities
// -----------------------------------------------------------------------------
#[inline]
fn get_neighbors(cell: u8) -> ([u8; 8], u8) {
    let mut neighbors = [0u8; 8];
    let mut count = 0;

    let r = (cell / 8) as i8;
    let c = (cell % 8) as i8;

    for dr in -1..=1 {
        for dc in -1..=1 {
            if dr == 0 && dc == 0 {
                continue;
            }
            let nr = r + dr;
            let nc = c + dc;
            if (0..8).contains(&nr) && (0..8).contains(&nc) {
                neighbors[count] = (nr * 8 + nc) as u8;
                count += 1;
            }
        }
    }
    (neighbors, count as u8)
}

impl BoardState {
    pub fn new(seed: u64, num_bombs: u8, rules: Rules) -> Self {
        let mut rng = SplitMix64::new(seed);
        let mut mine_mask: u64 = 0;
        let mut placed: u8 = 0;

        // Fisher-Yates style index sampling on a 64-element board
        let target_bombs = num_bombs.min(63);
        while placed < target_bombs {
            let candidate = rng.next_bounded(64) as u8;
            let bit = 1u64 << candidate;
            if (mine_mask & bit) == 0 {
                mine_mask |= bit;
                placed += 1;
            }
        }

        let mut state = Self {
            seed,
            num_bombs: target_bombs,
            move_count: 0,
            strikes: 0,
            rules,
            mine_mask,
            adj_counts: [0; 64],
            revealed_mask: 0,
            flagged_mask: 0,
            detonated_mask: 0,
            is_terminal: false,
            is_won: false,
        };

        state.recalculate_adj_counts();
        state
    }

    /// Builds a board with bombs located at exactly the set bits of `mine_mask`.
    ///
    /// Unlike [`BoardState::new`], this bypasses the PRNG, so the layout is
    /// pinned by `mine_mask` rather than a seed (`seed` is stored as `0`).
    /// `num_bombs` is derived from the popcount of `mine_mask`, and the
    /// adjacency counts are recomputed from the mask.
    ///
    /// This is primarily useful for deterministic test/eval fixtures, replays,
    /// curricula, and benchmarks where a specific layout must be reproduced
    /// exactly. A mask of `0` is a bomb-free board; a full mask has no safe
    /// cells and can never be won.
    pub fn from_mine_mask(mine_mask: u64, rules: Rules) -> Self {
        let mut state = Self {
            seed: 0,
            num_bombs: mine_mask.count_ones() as u8,
            move_count: 0,
            strikes: 0,
            rules,
            mine_mask,
            adj_counts: [0; 64],
            revealed_mask: 0,
            flagged_mask: 0,
            detonated_mask: 0,
            is_terminal: false,
            is_won: false,
        };

        state.recalculate_adj_counts();
        state
    }

    fn recalculate_adj_counts(&mut self) {
        self.adj_counts = [0; 64];
        for i in 0..64 {
            let (neighbors, n_count) = get_neighbors(i);
            let mut count = 0;
            for &n in &neighbors[..n_count as usize] {
                if (self.mine_mask & (1u64 << n)) != 0 {
                    count += 1;
                }
            }
            self.adj_counts[i as usize] = count;
        }
    }

    /// Safe-First-Click Relocation:
    /// If move #1 hits a bomb, relocate that single bomb to the first available
    /// empty cell starting at index 0, and update adjacent neighbor counts.
    fn relocate_mine(&mut self, hit_cell: u8) {
        self.mine_mask &= !(1u64 << hit_cell);

        for candidate in 0..64 {
            let candidate_bit = 1u64 << candidate;
            if candidate != hit_cell && (self.mine_mask & candidate_bit) == 0 {
                self.mine_mask |= candidate_bit;
                break;
            }
        }
        self.recalculate_adj_counts();
    }

    pub fn step(&mut self, action: Action) -> StepResult {
        if self.is_terminal || action.cell >= 64 {
            return StepResult {
                valid_move: false,
                newly_revealed: 0,
                hit_bomb: false,
                terminal: self.is_terminal,
                won: self.is_won,
            };
        }

        let cell_bit = 1u64 << action.cell;

        match action.action_type {
            ActionType::Flag => {
                // Cannot flag an already revealed cell
                if (self.revealed_mask & cell_bit) != 0 {
                    return StepResult {
                        valid_move: false,
                        newly_revealed: 0,
                        hit_bomb: false,
                        terminal: false,
                        won: false,
                    };
                }
                // Toggle flag
                self.flagged_mask ^= cell_bit;
                self.move_count += 1;

                StepResult {
                    valid_move: true,
                    newly_revealed: 0,
                    hit_bomb: false,
                    terminal: false,
                    won: false,
                }
            }

            ActionType::Reveal => {
                // Cannot reveal a flagged or already revealed cell
                if ((self.revealed_mask | self.flagged_mask) & cell_bit) != 0 {
                    return StepResult {
                        valid_move: false,
                        newly_revealed: 0,
                        hit_bomb: false,
                        terminal: false,
                        won: false,
                    };
                }

                // Move 1 rule: guaranteed safe initial click
                if self.move_count == 0 && (self.mine_mask & cell_bit) != 0 {
                    self.relocate_mine(action.cell);
                }

                self.move_count += 1;

                // Did we click a bomb?
                if (self.mine_mask & cell_bit) != 0 {
                    self.strikes += 1;
                    self.detonated_mask |= cell_bit;
                    self.revealed_mask |= cell_bit;

                    if self.strikes >= self.rules.max_strikes {
                        self.is_terminal = true;
                        self.is_won = false;
                    }

                    return StepResult {
                        valid_move: true,
                        newly_revealed: 1,
                        hit_bomb: true,
                        terminal: self.is_terminal,
                        won: false,
                    };
                }

                // Safe cell reveal with BFS cascade
                let mut queue = [0u8; 64];
                let mut head = 0;
                let mut tail = 0;

                self.revealed_mask |= cell_bit;
                let mut newly_revealed = 1u8;

                if self.adj_counts[action.cell as usize] == 0 {
                    queue[tail] = action.cell;
                    tail += 1;

                    while head < tail {
                        let curr = queue[head];
                        head += 1;

                        let (neighbors, n_count) = get_neighbors(curr);
                        for &n in &neighbors[..n_count as usize] {
                            let n_bit = 1u64 << n;
                            // Cascade reveals unrevealed, non-flagged, non-mine cells
                            if (self.revealed_mask & n_bit) == 0 && (self.flagged_mask & n_bit) == 0
                            {
                                self.revealed_mask |= n_bit;
                                newly_revealed += 1;

                                if self.adj_counts[n as usize] == 0 {
                                    queue[tail] = n;
                                    tail += 1;
                                }
                            }
                        }
                    }
                }

                // Check victory condition: all safe cells opened
                let total_safe = 64 - self.num_bombs as u32;
                let safe_revealed = (self.revealed_mask & !self.mine_mask).count_ones();

                if safe_revealed >= total_safe {
                    self.is_terminal = true;
                    self.is_won = true;
                }

                StepResult {
                    valid_move: true,
                    newly_revealed,
                    hit_bomb: false,
                    terminal: self.is_terminal,
                    won: self.is_won,
                }
            }
        }
    }

    /// Encodes the current visible state for neural network / ONNX consumption.
    /// Values:
    ///  -1.0 = Unrevealed (hidden)
    ///  -2.0 = Detonated bomb
    ///  -0.5 = Flagged
    ///   0.0 .. 0.8 = Exposed safe count (count / 10.0)
    pub fn get_observation(&self, out: &mut [f32; 64]) {
        for (i, slot) in out.iter_mut().enumerate() {
            let bit = 1u64 << i;
            *slot = if (self.detonated_mask & bit) != 0 {
                -2.0
            } else if (self.flagged_mask & bit) != 0 {
                -0.5
            } else if (self.revealed_mask & bit) == 0 {
                -1.0
            } else {
                (self.adj_counts[i] as f32) / 10.0
            };
        }
    }

    /// Returns a binary mask (1.0 = valid, 0.0 = invalid) for reveal actions [0..63].
    pub fn get_action_mask(&self, mask_out: &mut [f32; 64]) {
        for (i, slot) in mask_out.iter_mut().enumerate() {
            let bit = 1u64 << i;
            let is_open_or_flagged = (self.revealed_mask | self.flagged_mask) & bit != 0;
            *slot = if is_open_or_flagged { 0.0 } else { 1.0 };
        }
    }
}
