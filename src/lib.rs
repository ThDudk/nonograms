use std::fmt::{Display, Formatter, Write};
use std::ops::{Index, IndexMut, Range};
use std::path::Iter;
use std::process::Output;
use ndarray::{Array2, ArrayBase, ArrayView1, ArrayView2, Ix1};
use serde::{Deserialize, Serialize};

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

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum TileState {
    #[default]
    Empty,
    Crossed,
    Filled
}

pub struct NonogramBoard(Array2<TileState>);
impl NonogramBoard {
    pub fn new(matrix: Array2<TileState>) -> Self {
        Self(matrix)
    }
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

    pub fn clues(&self) -> NonogramClues {
        NonogramClues {
            row_clues: self.rows().map(NonogramClues::count_clues).collect(),
            col_clues: self.cols().map(NonogramClues::count_clues).collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineDir { Row, Col }
#[derive(Clone, Debug, PartialEq, Eq)]
struct BoardIdx(usize, usize);
impl BoardIdx {
    fn new(row: usize, col: usize) -> Self {
        Self(row, col)
    }
    fn directional(dir: LineDir, idx_along: usize, idx_perp: usize) -> Self {
        match dir {
            LineDir::Row => Self::new(idx_perp, idx_along),
            LineDir::Col => Self::new(idx_along, idx_perp),
        }
    }
    fn row(&self) -> usize {
        self.0
    }
    fn col(&self) -> usize {
        self.1
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
}


pub struct NonogramLine<'a>(ArrayView1<'a, TileState>);
impl<'a> NonogramLine<'a> {
    pub fn iter(&self) -> ndarray::iter::Iter<'_, TileState, Ix1> {
        self.0.iter()
    }
}