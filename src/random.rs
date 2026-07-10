use std::cmp::max;
use crate::NonogramBoard;

/// Generates a random `width` x `height` nonogram board.
///
/// `density` is the (rough) percentage of the board that is filled.
/// - `density = 1` means all tiles are filled.
/// - `density = 0` means no tiles are filled.
///
/// Tiles that are not filled use TileState::Crossed.
///
/// # Panics
///
/// - Panics if 0 <= density <= 1 is not satisfied.
/// - Panics if the board is too big: i.e. `width * height > usize::MAX`.
pub fn random_board(num_rows: usize, num_cols: usize, density: f64) -> NonogramBoard {
    if density < 0. || density > 1. {
        panic!("Expected density within bounds [0, 1]. found: {density}")
    }

    let vec = (0..num_rows * num_cols)
        .map(|_| rand::random_bool(density))
        .collect::<Vec<bool>>();

    NonogramBoard::from_binary_array((num_rows, num_cols), vec)
}

// TODO maybe a board iterator that stores whether it's solvable?

pub enum GenerationError {
    ExceededMaxAttempts
}

pub fn try_generate_solvable_board(max_attempts: u8, num_rows: usize, num_cols: usize, density: f64) -> Result<NonogramBoard, GenerationError> {
    for _ in 0..max_attempts {
        let board = random_board(num_rows, num_cols, density);

        let (_, solved) = crate::solver::blocking_logical_solver(&board.clues());
        if solved { return Ok(board); }
    }

    Err(GenerationError::ExceededMaxAttempts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TileState;
    use rstest::rstest;

    #[rstest]
    #[case(1000, 10000, 0.5, 0.001, 3)]
    #[case(10, 10, 0.5, 0.15, 100)]
    fn test_density(#[case] num_rows: usize, #[case] num_cols: usize, #[case] density: f64, #[case] tolerance: f64, #[case] repeats: usize) {
        for _ in 0..repeats {
            let board = random_board(num_rows, num_cols, density);

            let num_filled: usize = board.rows()
                .map(|row| {
                    row.iter()
                        .filter(|&&state| state == TileState::Filled)
                        .count()
                })
                .sum();

            let real_density = num_filled as f64 / (num_rows * num_cols) as f64;

            assert!(real_density - density < tolerance)
        }
    }

    #[rstest]
    #[should_panic] #[case(0, 0, -0.5)]
    #[should_panic] #[case(0, 0, 0.5)]
    #[should_panic] #[case(0, 10, 0.5)]
    #[should_panic] #[case(10, 0, 0.5)]
    #[should_panic] #[case(10, 10, -0.5)]
    #[should_panic] #[case(10, 10, 1.5)]
    #[should_panic] #[case(10, 10, 500.)]
    fn test_panics(#[case] width: usize, #[case] height: usize, #[case] density: f64) {
        random_board(width, height, density);
    }
}