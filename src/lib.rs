//! # Rust nonograms
//!
//! This crate provides 3 primary features:
//! - A structure to represent nonogram puzzles
//! - A logical solver which is significantly faster than similar crates
//! - Random puzzle generation (including generating solvable puzzles)
//!
//! # Example: Generating a random, solvable board
//!
//! ```
//! use nonograms::{random, NonogramBoard};
//!
//! let result = random::try_generate_solvable_board(3, 15, 15, 0.5);
//! if let Ok(board) = result {
//!     println!("{board}");
//! }
//! ```
//!
//! # Example: Using the logical solver
//!
//! ```
//! use nonograms::{random, solver, NonogramBoard};
//!
//! let result = random::try_generate_solvable_board(10, 15, 15, 0.9);
//!
//! if let Ok(board) = result {
//!     let nonogram_clues = board.clues();
//!
//!     let (solved_board, was_solved) = solver::blocking_logical_solver(&nonogram_clues);
//!
//!     assert!(was_solved);
//!     assert_eq!(board, solved_board);
//! }
//! ```

pub mod solver;
pub mod random;

use std::fmt::{Display, Formatter, Write};
use ndarray::{Array2, ArrayBase, ArrayView1, Ix1, Ix2, OwnedRepr};
use std::ops::{Index, IndexMut, Range};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(PartialEq, Eq, Debug)]
pub struct NonogramClues {
    pub row_clues: Vec<Vec<usize>>,
    pub col_clues: Vec<Vec<usize>>,
}
impl NonogramClues {
    pub fn row(&self, idx: usize) -> &Vec<usize> {
        &self.row_clues[idx]
    }
    pub fn col(&self, idx: usize) -> &Vec<usize> {
        &self.col_clues[idx]
    }

    fn count_clues(line: NonogramLine) -> Vec<usize> {
        let mut vec = line.iter().fold(
            vec![0_usize],
            |mut clues: Vec<usize>, tile| {
                match tile {
                    TileState::Filled => {
                        clues.last_mut().map(|clue| *clue += 1);
                    }
                    TileState::Crossed | TileState::Empty => {
                        clues.last()
                            .is_some_and(|clue| *clue != 0)
                            .then(|| clues.push(0));
                    }
                };
                clues
            }
        );

        if vec.last().is_some_and(|&len| len == 0) {
            vec.remove(vec.len() - 1);
        }

        vec
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum TileState {
    #[default]
    Empty,
    Crossed,
    Filled
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Eq, PartialEq, Debug)]
pub struct NonogramBoard(Array2<TileState>);
impl NonogramBoard { // TODO make NonogramBoard more strict in the size of the board (should be in u32)
    pub fn empty_square(side_len: usize) -> Self {
        Self::from_matrix(Array2::default((side_len, side_len)))
    }
    pub fn from_row_major(width: usize, height: usize, array: Vec<TileState>) -> Self {
        assert_ne!(width, 0, "Width cannot be 0");
        assert_ne!(height, 0, "Height cannot be 0");
        assert_eq!(width * height, array.len(), "Array length does not match width and height.");

        let matrix = Array2::from_shape_vec((height, width), array);

        Self::from_matrix(
            matrix.expect("Invalid array.")
        )
    }
    pub fn from_2d_vec(array: Vec<Vec<TileState>>) -> Self {
        let rows = array.len();
        let cols = array.get(0).map(|row| row.len()).unwrap_or(0);

        Self::from_row_major(cols, rows, array.into_iter().flatten().collect())
    }
    pub(crate) fn from_matrix(matrix: ArrayBase<OwnedRepr<TileState>, Ix2>) -> Self {
        Self(matrix)
    }
    pub(crate) fn from_binary_array((nrows, ncols): (usize, usize), binary_array: Vec<bool>) -> Self {
        assert_ne!(nrows, 0, "Cannot have 0 rows.");
        assert_ne!(ncols, 0, "Cannot have 0 columns.");

        let tile_state_vec = binary_array.into_iter()
            .map(|state|
                match state {
                    true => TileState::Filled,
                    false => TileState::Crossed,
                }
            ).collect();

        Self::from_matrix(
            Array2::from_shape_vec((nrows, ncols), tile_state_vec)
                .expect("binary array should have length: nrows * ncols.")
        )
    }

    /// # Returns
    pub fn rows(&'_ self) -> impl Iterator<Item = NonogramLine<'_>> {
        (0..self.0.nrows()).map(|idx| self.row(idx))
    }
    pub fn cols(&'_ self) -> impl Iterator<Item = NonogramLine<'_>> {
        (0..self.0.ncols()).map(|idx| self.col(idx))
    }
    pub fn row(&'_ self, idx: usize) -> NonogramLine<'_>{
        NonogramLine(self.0.row(idx))
    }
    pub fn col(&'_ self, idx: usize) -> NonogramLine<'_> {
        NonogramLine(self.0.column(idx))
    }

    pub fn width(&self) -> u32 {
        self.0.ncols() as u32
    }
    pub fn height(&self) -> u32 {
        self.0.nrows() as u32
    }

    pub fn clues(&self) -> NonogramClues {
        NonogramClues {
            row_clues: self.rows().map(NonogramClues::count_clues).collect(),
            col_clues: self.cols().map(NonogramClues::count_clues).collect(),
        }
    }
}
impl Index<BoardIdx> for NonogramBoard {
    type Output = TileState;

    fn index(&self, index: BoardIdx) -> &Self::Output {
        &self.0[(index.row(), index.col())]
    }
}
impl IndexMut<BoardIdx> for NonogramBoard {
    fn index_mut(&mut self, index: BoardIdx) -> &mut Self::Output {
        &mut self.0[(index.row(), index.col())]
    }
}
impl Display for NonogramBoard {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for row in self.rows() {
            for tile in row.iter() {
                let char = match tile {
                    TileState::Empty => '□',
                    TileState::Crossed => '☒',
                    TileState::Filled => '■',
                };
                f.write_char(char)?;
                f.write_char(' ')?;
            }
            write!(f, "\n")?;
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineDir { Row, Col }
impl LineDir {
    pub fn perp(&self) -> LineDir {
        match self {
            LineDir::Row => LineDir::Col,
            LineDir::Col => LineDir::Row,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoardIdx(usize, usize);
impl BoardIdx {
    pub fn new(row: usize, col: usize) -> Self {
        Self(row, col)
    }
    pub fn directional(dir: LineDir, idx_along: usize, idx_perp: usize) -> Self {
        match dir {
            LineDir::Row => Self::new(idx_perp, idx_along),
            LineDir::Col => Self::new(idx_along, idx_perp),
        }
    }
    pub fn row(&self) -> usize {
        self.0
    }
    pub fn col(&self) -> usize {
        self.1
    }
    pub fn in_dir(&self, dir: LineDir) -> usize {
        match dir {
            LineDir::Row => self.row(),
            LineDir::Col => self.col(),
        }
    }
    pub fn transposed(mut self, dir: LineDir, dist: i64) -> Self {
        match dir {
            LineDir::Row => {
                let col = self.1;
                self.1 = (col as i64 + dist) as usize;
            }
            LineDir::Col => {
                let row = self.0;
                self.0 = (row as i64 + dist) as usize;
            }
        }
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LineRange {
    dir: LineDir,
    line_idx: usize,
    range_along: Range<usize>,
}
impl LineRange {
    pub fn new(dir: LineDir, line_idx: usize, range_along: Range<usize>) -> Self {
        Self {
            dir,
            line_idx,
            range_along,
        }
    }

    pub fn start(&self) -> BoardIdx {
        BoardIdx::directional(self.dir, self.range_along.start, self.line_idx)
    }
    pub fn end(&self) -> BoardIdx {
        BoardIdx::directional(self.dir, self.range_along.end, self.line_idx)
    }
    pub fn iter(&self) -> impl Iterator<Item = BoardIdx> {
        self.range_along.clone()
            .map(|idx_along| BoardIdx::directional(self.dir, idx_along, self.line_idx))
    }

}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[derive(Debug)]
pub struct NonogramLine<'a>(ArrayView1<'a, TileState>);
impl<'a> NonogramLine<'a> {
    pub fn iter(&self) -> ndarray::iter::Iter<'_, TileState, Ix1> {
        self.0.iter()
    }

    pub fn get(&self, index: usize) -> Option<&TileState> {
        self.0.get(index)
    }

    pub fn len(&self) -> usize {
        self.0.iter().len()
    }
}
impl Index<usize> for NonogramLine<'_> {
    type Output = TileState;

    fn index(&self, index: usize) -> &Self::Output {
        &self.0[index]
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;
    use crate::{NonogramBoard, TileState};
    use crate::TileState::{Empty, Filled, Crossed};

    #[rstest]
    #[case(
        NonogramBoard::from_row_major(
            3, 3,
            vec![
                Filled, Filled, Empty,
                Crossed, Filled, Filled,
                Filled, Empty, Filled,
            ]
        ),
        "■ ■ □ \n☒ ■ ■ \n■ □ ■ \n"
    )]
    fn test_display(#[case] board: NonogramBoard, #[case] str: &'static str) {
        let string = format!("{board}");
        assert_eq!(string, str);
    }

    #[test]
    fn test_empty_square() {
        let board = NonogramBoard::empty_square(5);

        assert_eq!(5, board.width());
        assert_eq!(5, board.height());

        assert!(board.rows().all(|row| row.iter().all(|tile| *tile == Empty)))
    }
}