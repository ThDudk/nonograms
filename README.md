# Rust nonograms

This crate provides 3 primary features:
- A structure to represent nonogram puzzles
- A logical solver which is significantly faster than similar crates
- Random puzzle generation (including generating solvable puzzles)

# Usage

TODO

# Example: Generating a random, solvable board

```
use nonograms::{random, NonogramBoard};

let result = random::try_generate_solvable_board(3, 15, 15, 0.5);
if let Ok(board) = result {
    println!("{board}");
}
```

# Example: Using the logical solver

```
use nonograms::{random, solver, NonogramBoard};

let result = random::try_generate_solvable_board(10, 15, 15, 0.9);

if let Ok(board) = result {
    let nonogram_clues = board.clues();

    let (solved_board, was_solved) = solver::blocking_logical_solver(&nonogram_clues);

    assert!(was_solved);
    assert_eq!(board, solved_board);
}
```

# Async

TODO

# Performance

TODO
