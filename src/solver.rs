use crate::{BoardIdx, LineDir, LineRange, NonogramBoard, NonogramClues, NonogramLine, TileState};
use itertools::Itertools;
use ndarray::Array2;
use std::ops::{Add, Index, IndexMut, Range};
use std::{cmp, mem};

type LineIdx = usize;
type ClueIdx = usize;

#[derive(PartialEq, Eq, Debug, Clone)]
struct GlobalClueIdx(LineDir, LineIdx, ClueIdx);
impl GlobalClueIdx {
    fn line_dir(&self) -> LineDir {
        self.0
    }
    fn line_idx(&self) -> LineIdx {
        self.1
    }
    fn clue_idx(&self) -> ClueIdx {
        self.2
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ClueSpan {
    Filled {
        filled_span: Range<usize>,
        span: Range<usize>,
    },
    Single(Range<usize>),
    Multi(Vec<Range<usize>>),
}

impl ClueSpan {
    fn continuous_range(&self) -> Option<&Range<usize>> {
        match self {
            ClueSpan::Filled { span, .. } | ClueSpan::Single(span) => Some(span),
            ClueSpan::Multi(_) => None,
        }
    }

    fn is_filled_at(&self, idx: usize) -> bool {
        if let ClueSpan::Filled { filled_span, .. } = self {
            filled_span.contains(&idx)
        } else {
            false
        }
    }
    /// Precondition: Range is not 0
    fn is_filled_in_range(&self, range: Range<usize>) -> bool {
        debug_assert!(range.len() > 0, "Precondition is that range is not zero.");

        if let ClueSpan::Filled { filled_span, .. } = self {
            filled_span.contains(&range.start) && filled_span.contains(&(range.end - 1))
        } else {
            false
        }
    }
    fn contains(&self, idx: usize) -> bool {
        match self {
            ClueSpan::Filled { span, .. } | ClueSpan::Single(span) => {
                span.contains(&idx)
            },
            ClueSpan::Multi(spans) => {
                spans.iter().any(|span| span.contains(&idx))
            }
        }
    }

    fn overlapped(&self, range: Range<usize>) -> Option<Self> {
        match self {
            ClueSpan::Filled { span, filled_span } => {
                let start = cmp::max(span.start, range.start);
                let end = cmp::min(span.end, range.end);

                let filled_span = filled_span.clone(); // cheap cloning (2 usizes)
                let new_range = start..end;

                debug_assert!(
                    filled_span.start >= new_range.start && filled_span.end <= new_range.end,
                    "overlap has resulted in a clue span that is smaller then it's identified filled tiles. Filled: {filled_span:?}, new_range: {new_range:?}"
                );

                Some(ClueSpan::Filled { filled_span, span: new_range })
            },
            ClueSpan::Single(span) => {
                let start = cmp::max(span.start, range.start);
                let end = cmp::min(span.end, range.end);

                Some(ClueSpan::Single(start..end))
            }
            ClueSpan::Multi(spans) => {
                let mut spans = spans.clone().into_iter()
                    .filter(|span| {
                        !(span.end < range.start || span.start >= range.end)
                    })
                    .map(|span_range| {
                        let start = cmp::max(span_range.start, range.start);
                        let end = cmp::min(span_range.end, range.end);
                        start..end
                    })
                    .filter(|span| span.len() > 0)
                    .collect::<Vec<Range<usize>>>();

                match spans.len() {
                    0 => {None}
                    1 => {Some(ClueSpan::Single(spans.pop().unwrap()))}
                    2.. => {Some(ClueSpan::Multi(spans))}
                }
            }
        }
    }
    fn trim_spans_below_len(&self, len: usize) -> ClueSpan {
        let ClueSpan::Multi(spans) = self else { return self.clone() };

        let mut spans = spans.iter()
            .filter(|span| span.len() >= len)
            .map(|span| span.clone())
            .collect::<Vec<Range<usize>>>();

        debug_assert!(spans.len() > 0, "Trim resulted in span with 0 length.");

        if spans.len() == 1 {
            let span = spans.remove(0);
            ClueSpan::Single(span)
        } else {
            ClueSpan::Multi(spans)
        }
    }
}

#[derive(Clone, Debug)]
struct Clue {
    len: usize,
    span: ClueSpan,
}
impl Clue {
    fn new_simple(clue_len: usize, range: Range<usize>) -> Self {
        Self {
            len: clue_len,
            span: ClueSpan::Single(range),
        }
    }

    fn len(&self) -> usize {
        self.len
    }
    fn span(&self) -> &ClueSpan {
        &self.span
    }

    fn is_completed(&self) -> bool {
        if let ClueSpan::Filled { filled_span, span } = &self.span {
            filled_span.len() == span.len()
        } else {
            false
        }
    }

    fn fill_at(&mut self, idx_along: usize){
        self.span = match &self.span {
            ClueSpan::Filled { filled_span, span } => {
                let mut filled_span = filled_span.clone();

                if idx_along + 1 > filled_span.end {
                    filled_span.end = idx_along + 1;
                }
                if idx_along < filled_span.start {
                    filled_span.start = idx_along;
                }

                debug_assert!(self.len >= filled_span.len(), "Illegal state: filled area is larger than self.len()");
                ClueSpan::Filled {
                    filled_span,
                    span: span.clone(),
                }
            }
            ClueSpan::Single(span) => {
                ClueSpan::Filled {
                    filled_span: idx_along..idx_along + 1,
                    span: span.clone(),
                }
            }
            ClueSpan::Multi(spans) => {
                let span_with_filled_idx = spans.iter()
                    .find(|span| span.contains(&idx_along))
                    .expect("idx_along being filled implies idx_along is within one possible span of this");

                ClueSpan::Filled {
                    filled_span: idx_along..idx_along + 1,
                    span: span_with_filled_idx.clone(),
                }
            }
        };

        self.update_span_based_on_filled();
    }
    fn cross_at(&mut self, idx_along: usize) {
        if !self.span.contains(idx_along) { return }

        self.span = match &self.span {
            ClueSpan::Filled { filled_span, span } => {
                let mut span = span.clone();

                if idx_along < filled_span.start { // use less than since filled_span.start is inclusive (can't cross and fill a tile at the same time)
                    span.start = cmp::max(idx_along + 1, span.start);
                }
                if idx_along >= filled_span.end {
                    span.end = cmp::min(idx_along, span.end);
                }

                debug_assert!(self.len >= filled_span.len(), "Illegal state: filled area is larger than self.len()");
                ClueSpan::Filled {
                    filled_span: filled_span.clone(),
                    span,
                }
            }
            ClueSpan::Single(span) => {
                let left = self.span.overlapped(span.start..idx_along) // don't include idx_along
                    .expect("There should be some overlap range")
                    .continuous_range()
                    .expect("left must be a simple range")
                    .clone();
                let right = self.span.overlapped(idx_along + 1..span.end) // don't include idx_along
                    .expect("There should be some overlap range")
                    .continuous_range()
                    .expect("right must be a simple range")
                    .clone();

                debug_assert!(left.len() > 0 || right.len() > 0, "It's impossible for left and right to be 0 for a simple range (as this would mean there is no valid spot).");

                if left.len() == 0 {
                    ClueSpan::Single(right)
                }
                else if right.len() == 0 {
                    ClueSpan::Single(left)
                }
                else {
                    ClueSpan::Multi(vec![left, right])
                }
            }
            ClueSpan::Multi(spans) => {
                let (idx, span) = spans.iter()
                    .find_position(|span| span.contains(&idx_along))
                    .expect("Function precondition is that idx_along is contained in this.");

                let left = self.span.overlapped(span.start..idx_along) // don't include idx_along
                    .map(|span| {
                        span.continuous_range()
                            .expect("left physically must be a simple range")
                            .clone()
                    });

                let right = self.span.overlapped(idx_along + 1..span.end) // don't include idx_along
                    .map(|span| {
                        span.continuous_range()
                            .expect("right physically must be a simple range")
                            .clone() // clone is cheap since it's a Range<usize>
                    });

                let mut spans = spans.clone();
                spans.swap_remove(idx);

                if left.is_some() { spans.push(left.unwrap().clone()) }
                if right.is_some() { spans.push(right.unwrap().clone()) }

                if spans.len() == 1 {
                    ClueSpan::Single(spans.pop().expect("Cannot have a clue span with 0 ranges."))
                } else {
                    ClueSpan::Multi(spans)
                }
            }
        }
    }
    fn update_span_based_on_filled(&mut self) {
        let ClueSpan::Filled{ filled_span, .. } = &mut self.span else {panic!("Expected filled span. Found {self:?}")};

        debug_assert!(self.len >= filled_span.len(), "Illegal state: filled area is larger than self.len()");

        let padding = self.len - filled_span.len();

        let min_pos = (padding < filled_span.start)
            .then(|| filled_span.start - padding)
            .unwrap_or(0); // prevents underflow
        let max_pos = filled_span.end + padding;

        self.span = self.span.overlapped(min_pos..max_pos).expect("Cannot have span with len 0"); // use overlapped to prevent it from expanding
        debug_assert!(max_pos - min_pos >= self.len(), "Illegal state: possible range is smaller than self.len()");
    }
}

#[derive(Debug)]
struct WorkingClues {
    row_clues: Vec<Vec<Clue>>,
    col_clues: Vec<Vec<Clue>>,
}
impl WorkingClues {
    fn row(&self, idx: usize) -> &Vec<Clue> {
        &self.row_clues[idx]
    }
    fn col(&self, idx: usize) -> &Vec<Clue> {
        &self.col_clues[idx]
    }
    fn line(&self, line_dir: LineDir, line_idx: usize) -> &Vec<Clue> {
        match line_dir {
            LineDir::Row => {self.row(line_idx)}
            LineDir::Col => {self.col(line_idx)}
        }
    }
}
impl Index<&GlobalClueIdx> for WorkingClues {
    type Output = Clue;

    fn index(&self, index: &GlobalClueIdx) -> &Self::Output {
        match index.line_dir() {
            LineDir::Row => {
                &self.row_clues[index.line_idx()][index.clue_idx()]
            }
            LineDir::Col => {
                &self.col_clues[index.line_idx()][index.clue_idx()]
            }
        }
    }
}
impl IndexMut<&GlobalClueIdx> for WorkingClues {
    fn index_mut(&mut self, index: &GlobalClueIdx) -> &mut Self::Output {
        match index.line_dir() {
            LineDir::Row => {
                &mut self.row_clues[index.line_idx()][index.clue_idx()]
            }
            LineDir::Col => {
                &mut self.col_clues[index.line_idx()][index.clue_idx()]
            }
        }
    }
}
impl From<&NonogramClues> for WorkingClues {
    fn from(value: &NonogramClues) -> Self {
        let width = value.col_clues.len();
        let height = value.row_clues.len();

        Self {
            row_clues: value.row_clues.clone().iter()
                .map(|clues| {
                    clues.into_iter().map(|clue| {
                        Clue::new_simple(*clue, 0..height)
                    }).collect()
                }).collect(),

            col_clues: value.col_clues.clone().iter()
                .map(|clues| {
                    clues.into_iter().map(|clue| {
                        Clue::new_simple(*clue, 0..width)
                    }).collect()
                }).collect(),
        }
    }
}


#[derive(PartialEq, Eq, Debug, Clone)]
enum SolverAction {
    UpdateClueSpan(ClueSpan, GlobalClueIdx),
    FillTile(BoardIdx, GlobalClueIdx),
    FillRange(LineRange, GlobalClueIdx),
    CrossTile(BoardIdx, Vec<GlobalClueIdx>),
}

#[derive(Default, Debug)]
struct SolverCtx {
    actions: Vec<SolverAction>,
}
impl SolverCtx {
    fn set_line(&mut self, line_dir: LineDir, line_idx: LineIdx) -> LineSolverContext<'_> {
        LineSolverContext {
            solver_ctx: self,
            line_dir,
            line_idx,
        }
    }
    fn num_mutations(&self) -> usize {
        self.actions.len()
    }

    fn flush(&mut self, board: &mut NonogramBoard, working_clues: &mut WorkingClues) {
        let actions = mem::take(&mut self.actions);

        for action in actions {
            match action {
                SolverAction::UpdateClueSpan(new_span, clue_idx) => {
                    working_clues[&clue_idx].span = new_span;
                }
                SolverAction::FillTile(board_idx, clue_idx) => {
                    let idx_along = board_idx.in_dir(clue_idx.line_dir().perp());

                    board[board_idx] = TileState::Filled;
                    working_clues[&clue_idx].fill_at(idx_along);
                }
                SolverAction::FillRange(line_range, clue_idx) => {
                    let start_pos = line_range.start();
                    let end_pos = line_range.end().transposed(line_range.dir, -1);

                    [start_pos, end_pos].into_iter().for_each(|board_idx| {
                        let idx_along = board_idx.in_dir(clue_idx.line_dir().perp());

                        working_clues[&clue_idx].fill_at(idx_along);
                    });

                    line_range.iter().for_each(|board_idx| {
                        board[board_idx] = TileState::Filled;
                    })
                }
                SolverAction::CrossTile(board_idx, clues) => {
                    board[board_idx] = TileState::Crossed;

                    if clues.len() > 0 {
                        let idx_along = board_idx.in_dir(clues[0].line_dir().perp());

                        clues.iter().for_each(|clue_idx| {
                            working_clues[clue_idx].cross_at(idx_along)
                        });
                    }
                }
            }
        }
    }
}

#[derive(Debug)]
struct LineSolverContext<'a> {
    solver_ctx: &'a mut SolverCtx,
    line_dir: LineDir,
    line_idx: LineIdx,
}
impl<'a> LineSolverContext<'a> {
    fn to_board_idx(&self, idx_along: usize) -> BoardIdx {
        BoardIdx::directional(self.line_dir, idx_along, self.line_idx)
    }
    fn to_line_range(&self, range: Range<usize>) -> LineRange {
        LineRange::new(self.line_dir, self.line_idx, range)
    }
    fn to_global_clue_idx(&self, clue_idx: ClueIdx) -> GlobalClueIdx {
        GlobalClueIdx(self.line_dir, self.line_idx, clue_idx)
    }

    fn push_action(&mut self, action: SolverAction) {
        self.solver_ctx.actions.push(action);
    }

    fn update_clue_span(&mut self, new_span: ClueSpan, clue_idx: ClueIdx) {
        self.solver_ctx.actions.push(SolverAction::UpdateClueSpan(
            new_span,
            self.to_global_clue_idx(clue_idx)
        ))
    }

    fn fill_tile(&mut self, idx: usize, clue_idx: ClueIdx) {
        let tile = self.to_board_idx(idx);
        let action = SolverAction::FillTile(
            tile,
            self.to_global_clue_idx(clue_idx)
        );
        self.push_action(action);
    }
    fn fill_range(&mut self, range: Range<usize>, clue_idx: ClueIdx) {
        let range = self.to_line_range(range);
        let action = SolverAction::FillRange(
            range,
            self.to_global_clue_idx(clue_idx)
        );
        self.push_action(action);
    }
    fn cross_tile(&mut self, idx: usize, clues_at_each_pos: &Vec<Vec<ClueIdx>>) {
        let tile = self.to_board_idx(idx);
        let clues_vec = clues_at_each_pos[idx].iter()
            .map(|&clue_idx| {
                self.to_global_clue_idx(clue_idx)
            })
            .collect();

        let action = SolverAction::CrossTile(tile, clues_vec);
        self.push_action(action);
    }
}

/// Attempts to solve the given nonogram clues using a logical solver.
///
/// This functions blocks the current thread until completion, which could last a long time depending on the board size.
pub fn blocking_logical_solver(clues: &NonogramClues) -> (NonogramBoard, bool) {
    let num_cols = clues.col_clues.len();
    let num_rows = clues.row_clues.len();

    let array = Array2::from_elem((num_rows, num_cols), TileState::Empty);
    let mut board = NonogramBoard(array);

    let mut working_clues: WorkingClues = clues.into();

    let mut ctx = SolverCtx::default();

    let mut loop_count = 0;
    loop {
        let mut mutations = 0;

        board.rows().enumerate().for_each(|(line_idx, line)|
            line_pass(&mut ctx, &line, &working_clues, LineDir::Row, line_idx)
        );
        mutations += ctx.num_mutations();
        ctx.flush(&mut board, &mut working_clues);

        board.cols().enumerate().for_each(|(line_idx, line)|
            line_pass(&mut ctx, &line, &working_clues, LineDir::Col, line_idx)
        );
        mutations += ctx.num_mutations();
        ctx.flush(&mut board, &mut working_clues);

        if mutations == 0 {
            break
        }

        loop_count += 1;
        if loop_count > 50 * num_cols * num_rows {
            println!("WARN: Pass limit reached. Looped: {} times, which means something likely went wrong. Ending loop.", loop_count);
            break
        }
    }

    let solved = board.rows().all(|row| {
        row.iter().all(|&tile| {
            tile != TileState::Empty
        })
    });

    (board, solved)
}

// TODO async solver

fn line_pass(ctx: &mut SolverCtx, line: &NonogramLine, working_clues: &WorkingClues, dir: LineDir, line_idx: LineIdx) {
    let mut ctx = ctx.set_line(dir, line_idx);

    let clues = working_clues.line(dir, line_idx);

    let mut clues_at_each_pos: Vec<Vec<ClueIdx>> = vec![];

    // possible optimization: use a sliding window?
    for pos in 0..line.len() {
        let clues_at_pos = clues.iter().enumerate()
            .filter(|(_idx, clue)| clue.span().contains(pos))
            .map(|(idx, _clue)| idx).collect();

        clues_at_each_pos.push(clues_at_pos)
    }

    overlap_with_most_liberal_spans(&mut ctx, clues, &clues_at_each_pos);
    overlap(&mut ctx, clues);
    cross_tiles_with_no_spans(&mut ctx, line, &clues_at_each_pos);
    cross_next_to_completed(&mut ctx, line, &clues, &clues_at_each_pos);
    identify_filled_tiles_when_theres_one_span(&mut ctx, line, clues, &clues_at_each_pos);
    cross_crossed_tiles(&mut ctx, line, &clues_at_each_pos);
}

fn overlap(ctx: &mut LineSolverContext, clues: &[Clue]) {
    clues.iter().enumerate()
        .for_each(|(clue_idx, clue)| {
            let overlap_range = match &clue.span {
                ClueSpan::Filled { span, .. } | ClueSpan::Single(span) => {
                    let point_from_end = span.end - clue.len;
                    let point_from_start = span.start + clue.len;

                    (point_from_end < point_from_start).then_some(point_from_end..point_from_start)
                },
                ClueSpan::Multi(_) => { None }
            };

            if let Some(range) = overlap_range {
                if clue.span.is_filled_in_range(range.clone()) { return }; // avoid adding unnecessary actions

                ctx.fill_range(range, clue_idx)
            };
        });
}
fn overlap_with_most_liberal_spans(ctx: &mut LineSolverContext, clues: &[Clue], clues_at_each_position: &Vec<Vec<ClueIdx>>) {
    if clues.len() == 0 { return }

    let start_inclusive = clues_at_each_position.iter().enumerate()
        .find_position(|(_, clues)| !clues.is_empty())
        .expect("there must be a clue somewhere").0;

    let end_inclusive = clues_at_each_position.iter().enumerate().rev()
        .find(|(_, clues)| !clues.is_empty())
        .expect("there must be a clue somewhere").0;

    let line_len = (end_inclusive + 1) - start_inclusive;

    // Clues at each position should no longer be used.

    let clue_spaces_sum = if clues.len() > 0 {
        clues.iter()
            .map(|clue| clue.len())
            .sum::<usize>()
            .add(clues.len() - 1)
    } else { 0 };

    clues.iter()
        .scan(
            0_usize,
            |clue_spaces_before, clue| {
                let clue_spaces_sum_after = clue_spaces_sum - *clue_spaces_before - clue.len();
                let liberal_span_range = *clue_spaces_before..(line_len - clue_spaces_sum_after);

                *clue_spaces_before += clue.len() + 1;
                return Some((liberal_span_range, clue)) // returns Some bc scan is dumb
            }
        )
        .enumerate()
        .map(|(clue_idx, (liberal_span_range, clue))| (liberal_span_range, clue, clue_idx))
        .filter(|(liberal_span_range, clue, _clue_idx)| {
            !clue.span.continuous_range().is_some_and(|range| {
                range.start >= liberal_span_range.start && range.end <= liberal_span_range.end
            })
        })
        // offset liberal span range so it's in the correct coordinates again
        .map(|(liberal_span_range, clue, clue_idx)| (
            liberal_span_range.start + start_inclusive..liberal_span_range.end + start_inclusive,
            clue,
            clue_idx
        ))
        .for_each(|(liberal_span_range, clue, clue_idx)| {
            let new_span = clue.span
                .overlapped(liberal_span_range).expect("Cannot have span with no length")
                .trim_spans_below_len(clue.len);

            if clue.span != new_span {
                ctx.update_clue_span(new_span, clue_idx)
            }
        });
}
fn cross_tiles_with_no_spans(ctx: &mut LineSolverContext, board_line: &NonogramLine, clues_at_each_position: &Vec<Vec<ClueIdx>>) {
    clues_at_each_position.iter().enumerate()
        .filter(|&(_pos_idx, _clues)|
            board_line[_pos_idx] != TileState::Crossed
        )
        .for_each(|(pos_idx, clues)| {
            if clues.len() == 0 {
                ctx.cross_tile(pos_idx, clues_at_each_position)
            }
        })
}
fn cross_next_to_completed(ctx: &mut LineSolverContext, board_line: &NonogramLine, clues: &[Clue], clues_at_each_position: &Vec<Vec<ClueIdx>>) {
    clues.iter()
        .filter(|clue| clue.is_completed())
        .for_each(|clue| {
            let range = clue.span.continuous_range()
                .expect("Since clue is completed, it must be Simple or Filled.");

            let tile_before = (range.start > 0).then(|| &board_line[range.start - 1]); // closure so it does not underflow
            let tile_after = board_line.get(range.end);

            if let Some(&tile_state) = tile_before && tile_state != TileState::Crossed {
                ctx.cross_tile(range.start - 1, clues_at_each_position)
            }
            if let Some(&tile_state) = tile_after && tile_state != TileState::Crossed {
                ctx.cross_tile(range.end, clues_at_each_position)
            }
        });
}
fn identify_filled_tiles_when_theres_one_span(ctx: &mut LineSolverContext, board_line: &NonogramLine, clues: &[Clue], clues_at_each_position: &Vec<Vec<ClueIdx>>) {
    clues_at_each_position.iter().enumerate()
        .filter(|&(idx, _clue_indices)| board_line[idx] == TileState::Filled )
        .filter(|&(_idx, clue_indices)| clue_indices.len() == 1)
        .map(|(idx, clue_indices)| (idx, clue_indices[0]))
        .filter(|&(idx, clue_index)| !clues[clue_index].span.is_filled_at(idx))
        .for_each(|(idx, clue_idx)| {
            ctx.fill_tile(idx, clue_idx)
        })
}
fn cross_crossed_tiles(ctx: &mut LineSolverContext, board_line: &NonogramLine, clues_at_each_position: &Vec<Vec<ClueIdx>>) {
    board_line.iter().enumerate()
        .filter(|&(_idx, &state)| state == TileState::Crossed)
        .filter(|&(idx, _state)| !clues_at_each_position[idx].is_empty())
        .for_each(|(idx, _state)| {
            ctx.cross_tile(idx, clues_at_each_position);
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::Array1;
    use rstest::rstest;

    mod test_overlap {
        use super::*;

        #[test]
        fn full() {
            let clues = vec![
                Clue{ len: 5, span: ClueSpan::Single(0..5) }
            ];

            let mut context = SolverCtx::default();
            let mut line_ctx = context.set_line(LineDir::Row, 0);

            overlap(&mut line_ctx, &clues);

            assert_eq!(
                vec![
                    SolverAction::FillRange(
                        LineRange::new(LineDir::Row, 0, 0..5),
                        GlobalClueIdx(LineDir::Row, 0, 0)
                    ),
                ],
                context.actions
            )
        }
        #[test]
        fn partial() {
            let clues = vec![
                Clue{ len: 3, span: ClueSpan::Single(0..5) }
            ];

            let mut context = SolverCtx::default();
            let mut line_ctx = context.set_line(LineDir::Row, 0);

            overlap(&mut line_ctx, &clues);

            assert_eq!(
                vec![
                    SolverAction::FillRange(
                        LineRange::new(LineDir::Row, 0, 2..3),
                        GlobalClueIdx(LineDir::Row, 0, 0)
                    ),
                ],
                context.actions
            )
        }
        #[test]
        fn gives_nothing_from_checker_pattern() {
            let clues = vec![
                Clue{ len: 1, span: ClueSpan::Single(0..3) }
            ];

            let mut context = SolverCtx::default();
            let mut line_ctx = context.set_line(LineDir::Row, 0);

            overlap(&mut line_ctx, &clues);

            println!("{:?}", context.actions);
            assert!(context.actions.is_empty())
        }
    }
    mod test_overlap_with_most_liberal_spans {
        use super::*;

        #[test]
        fn single_spans() {
            let clues_at_each_pos: Vec<Vec<ClueIdx>> = vec![
                vec![01],
                vec![01],
                vec![01],
                vec![01],
                vec![01],
            ];

            let clues = vec![
                Clue{ len: 2, span: ClueSpan::Single(0..5) },
                Clue{ len: 2, span: ClueSpan::Single(0..5) },
            ];

            let mut context = SolverCtx::default();
            let mut line_ctx = context.set_line(LineDir::Row, 0);

            overlap_with_most_liberal_spans(&mut line_ctx, &clues, &clues_at_each_pos);

            assert_eq!(
                vec![
                    SolverAction::UpdateClueSpan(
                        ClueSpan::Single(Range::from(0..2)),
                        GlobalClueIdx(LineDir::Row, 0, 0)
                    ),
                    SolverAction::UpdateClueSpan(
                        ClueSpan::Single(Range::from(3..5)),
                        GlobalClueIdx(LineDir::Row, 0, 1)
                    ),
                ],
                context.actions
            )
        }
        #[test]
        fn multi_spans() {
            let clues_at_each_pos: Vec<Vec<ClueIdx>> = vec![
                vec![01],
                vec![],
                vec![01],
                vec![],
                vec![],
            ];

            let clues = vec![
                Clue{ len: 1, span: ClueSpan::Multi(vec![0..1, 2..3]) },
                Clue{ len: 1, span: ClueSpan::Multi(vec![0..1, 2..3]) },
            ];

            let mut context = SolverCtx::default();
            let mut line_ctx = context.set_line(LineDir::Row, 0);

            overlap_with_most_liberal_spans(&mut line_ctx, &clues, &clues_at_each_pos);

            assert_eq!(
                vec![
                    SolverAction::UpdateClueSpan(
                        ClueSpan::Single(Range::from(0..1)),
                        GlobalClueIdx(LineDir::Row, 0, 0)
                    ),
                    SolverAction::UpdateClueSpan(
                        ClueSpan::Single(Range::from(2..3)),
                        GlobalClueIdx(LineDir::Row, 0, 1)
                    ),
                ],
                context.actions
            )
        }
        #[test]
        fn mix_of_spans() {
            // aaBaCCC
            // aa
            //   BB
            //     CCC

            let clues_at_each_pos: Vec<Vec<ClueIdx>> = vec![
                vec![0],
                vec![0],
                vec![1],
                vec![0],
                vec![2],
                vec![2],
                vec![2],
            ];

            let clues = vec![
                Clue{ len: 1, span: ClueSpan::Multi(vec![0..2, 3..4]) },
                Clue{ len: 1, span: ClueSpan::Filled{ filled_span: 2..3, span: 2..3 } },
                Clue{ len: 2, span: ClueSpan::Filled { filled_span: 5..6, span: 4..7 } },
            ];

            let mut context = SolverCtx::default();
            let mut line_ctx = context.set_line(LineDir::Row, 0);

            overlap_with_most_liberal_spans(&mut line_ctx, &clues, &clues_at_each_pos);

            assert_eq!(
                vec![
                    SolverAction::UpdateClueSpan(
                        ClueSpan::Single(Range::from(0..2)), // Note: this is unintuitively 0..2 NOT 0..1
                        GlobalClueIdx(LineDir::Row, 0, 0)
                    ),
                    // no update for B
                    // no update for C
                ],
                context.actions
            )
        }
    }
    mod test_cross_tiles_with_no_spans {
        use crate::solver::{cross_tiles_with_no_spans, ClueIdx, SolverAction, SolverCtx};
        use crate::TileState::Crossed;
        use crate::{BoardIdx, LineDir, NonogramLine, TileState};
        use ndarray::Array1;
        use TileState::Empty;

        #[test]
        fn simple() {
            // 2, 2 : 00X11
            let clues_at_each_pos: Vec<Vec<ClueIdx>> = vec![
                vec![0],
                vec![0],
                vec![],
                vec![1],
                vec![1],
            ];

            let line = Array1::from_vec(vec![Empty; 5]);
            let line = NonogramLine(line.view());

            let mut context = SolverCtx::default();
            let mut line_ctx = context.set_line(LineDir::Row, 0);

            cross_tiles_with_no_spans(&mut line_ctx, &line, &clues_at_each_pos);

            assert_eq!(
                vec![
                    SolverAction::CrossTile(
                        BoardIdx(0, 2),
                        vec![]
                    ),
                ],
                context.actions
            )
        }
        #[test]
        fn no_matches() {
            // 1, 1 : 00(01)11
            let clues_at_each_pos: Vec<Vec<ClueIdx>> = vec![
                vec![0],
                vec![0],
                vec![0, 1],
                vec![1],
                vec![1],
            ];

            let line = Array1::from_vec(vec![Empty; 5]);
            let line = NonogramLine(line.view());

            let mut context = SolverCtx::default();
            let mut line_ctx = context.set_line(LineDir::Row, 0);

            cross_tiles_with_no_spans(&mut line_ctx, &line, &clues_at_each_pos);

            assert!(context.actions.is_empty())
        }
        #[test]
        fn already_filled() {
            // 2, 2 : 00X11
            let clues_at_each_pos: Vec<Vec<ClueIdx>> = vec![
                vec![0],
                vec![0],
                vec![],
                vec![1],
                vec![1],
            ];

            let line = Array1::from_vec(vec![Empty, Empty, Crossed, Empty, Empty]);
            let line = NonogramLine(line.view());

            let mut context = SolverCtx::default();
            let mut line_ctx = context.set_line(LineDir::Row, 0);

            cross_tiles_with_no_spans(&mut line_ctx, &line, &clues_at_each_pos);

            assert!(context.actions.is_empty())
        }
    }
    mod test_cross_next_to_completed {
        use super::*;
        use crate::TileState::{Empty, Filled};

        #[test]
        fn does_nothing_if_there_are_no_completed_spans() {
            let clues_at_each_pos = vec![
                vec![],
                vec![0],
                vec![0],
                vec![0],
                vec![],
            ];

            let clues = vec![
                Clue{ len: 3, span: ClueSpan::Single(1..4) },
            ];

            let line = Array1::from_vec(vec![Empty; 5]);
            let line = NonogramLine(line.view());

            let mut context = SolverCtx::default();
            let mut line_ctx = context.set_line(LineDir::Row, 0);

            cross_next_to_completed(&mut line_ctx, &line, &clues, &clues_at_each_pos);

            // nothing should be done even though clues[0] fills the row
            // because the clue is a single span, not filled (and thus not completed).
            assert!(context.actions.is_empty())
        }
        #[test]
        fn does_not_fill_left_of_line_bounds() {
            let clues_at_each_pos = vec![
                vec![0],
                vec![0],
                vec![0],
                vec![],
                vec![],
            ];

            let clues = vec![
                Clue{
                    len: 3,
                    span: ClueSpan::Filled{ filled_span: 0..3, span: 0..3 }
                },
            ];

            let line = Array1::from_vec(vec![Filled, Filled, Filled, Empty, Empty]);
            let line = NonogramLine(line.view());

            let mut context = SolverCtx::default();
            let mut line_ctx = context.set_line(LineDir::Row, 0);

            cross_next_to_completed(&mut line_ctx, &line, &clues, &clues_at_each_pos);

            assert_eq!(
                vec![
                    SolverAction::CrossTile(
                        BoardIdx(0, 3),
                        vec![]
                    ),
                ],
                context.actions
            );
        }
        #[test]
        fn does_not_fill_right_of_line_bounds() {
            let clues_at_each_pos = vec![
                vec![],
                vec![],
                vec![0],
                vec![0],
                vec![0],
            ];

            let clues = vec![
                Clue{
                    len: 3,
                    span: ClueSpan::Filled{ filled_span: 2..5, span: 2..5 }
                },
            ];

            let line = Array1::from_vec(vec![Empty, Empty, Filled, Filled, Filled]);
            let line = NonogramLine(line.view());

            let mut context = SolverCtx::default();
            let mut line_ctx = context.set_line(LineDir::Row, 0);

            cross_next_to_completed(&mut line_ctx, &line, &clues, &clues_at_each_pos);

            assert_eq!(
                vec![
                    SolverAction::CrossTile(
                        BoardIdx(0, 1),
                        vec![]
                    )
                ],
                context.actions
            );
        }
        #[test]
        fn does_not_fill_out_of_line_bounds() {
            let clues_at_each_pos = vec![
                vec![0],
                vec![0],
                vec![0],
                vec![0],
                vec![0],
            ];
            let clues = vec![
                Clue{
                    len: 5,
                    span: ClueSpan::Filled{ filled_span: 0..5, span: 0..5 }
                },
            ];

            let line = Array1::from_vec(vec![Filled, Filled, Filled, Filled, Filled]);
            let line = NonogramLine(line.view());

            let mut context = SolverCtx::default();
            let mut line_ctx = context.set_line(LineDir::Row, 0);

            cross_next_to_completed(&mut line_ctx, &line, &clues, &clues_at_each_pos);

            assert!(context.actions.is_empty());
        }
    }
    mod test_identify_filled_tiles_when_theres_one_span {
        use super::*;
        use crate::TileState::{Empty, Filled};

        #[test]
        fn whole_line() {
            let clues = vec![
                Clue{ len: 5, span: ClueSpan::Single(0..5) }
            ];
            let clues_at_each_pos: Vec<Vec<ClueIdx>> = vec![
                vec![0],
                vec![0],
                vec![0],
                vec![0],
                vec![0],
            ];

            let line = Array1::from_vec(vec![Filled; 5]);
            let line = NonogramLine(line.view());

            let mut context = SolverCtx::default();
            let mut line_ctx = context.set_line(LineDir::Row, 0);

            identify_filled_tiles_when_theres_one_span(&mut line_ctx, &line, &clues, &clues_at_each_pos);

            assert_eq!(
                vec![
                    SolverAction::FillTile(BoardIdx(0, 0), GlobalClueIdx(LineDir::Row, 0, 0)),
                    SolverAction::FillTile(BoardIdx(0, 1), GlobalClueIdx(LineDir::Row, 0, 0)),
                    SolverAction::FillTile(BoardIdx(0, 2), GlobalClueIdx(LineDir::Row, 0, 0)),
                    SolverAction::FillTile(BoardIdx(0, 3), GlobalClueIdx(LineDir::Row, 0, 0)),
                    SolverAction::FillTile(BoardIdx(0, 4), GlobalClueIdx(LineDir::Row, 0, 0)),
                ],
                context.actions
            )
        }
        #[test]
        fn only_fills_tiles_with_one_option() {
            let clues = vec![
                Clue{ len: 1, span: ClueSpan::Single(0..3) },
                Clue{ len: 1, span: ClueSpan::Single(2..5) },
            ];
            let clues_at_each_pos: Vec<Vec<ClueIdx>> = vec![
                vec![0],
                vec![0],
                vec![0, 1],
                vec![1],
                vec![1],
            ];

            let line = Array1::from_vec(vec![Filled; 5]);
            let line = NonogramLine(line.view());

            let mut context = SolverCtx::default();
            let mut line_ctx = context.set_line(LineDir::Row, 0);

            identify_filled_tiles_when_theres_one_span(&mut line_ctx, &line, &clues, &clues_at_each_pos);

            assert_eq!(
                vec![
                    SolverAction::FillTile(BoardIdx(0, 0), GlobalClueIdx(LineDir::Row, 0, 0)),
                    SolverAction::FillTile(BoardIdx(0, 1), GlobalClueIdx(LineDir::Row, 0, 0)),
                    SolverAction::FillTile(BoardIdx(0, 3), GlobalClueIdx(LineDir::Row, 0, 1)),
                    SolverAction::FillTile(BoardIdx(0, 4), GlobalClueIdx(LineDir::Row, 0, 1)),
                ],
                context.actions
            )
        }
        #[test]
        fn handles_multi_ranges() {
            let clues = vec![
                Clue{ len: 1, span: ClueSpan::Multi(vec![0..1, 2..4]) },
                Clue{ len: 1, span: ClueSpan::Single(3..5) },
            ];
            let clues_at_each_pos: Vec<Vec<ClueIdx>> = vec![
                vec![0],
                vec![],
                vec![0],
                vec![0, 1],
                vec![1],
            ];

            let line = Array1::from_vec(vec![Filled, Empty, Filled, Filled, Filled]);
            let line = NonogramLine(line.view());

            let mut context = SolverCtx::default();
            let mut line_ctx = context.set_line(LineDir::Row, 0);

            identify_filled_tiles_when_theres_one_span(&mut line_ctx, &line, &clues, &clues_at_each_pos);

            assert_eq!(
                vec![
                    SolverAction::FillTile(BoardIdx(0, 0), GlobalClueIdx(LineDir::Row, 0, 0)),
                    SolverAction::FillTile(BoardIdx(0, 2), GlobalClueIdx(LineDir::Row, 0, 0)),
                    SolverAction::FillTile(BoardIdx(0, 4), GlobalClueIdx(LineDir::Row, 0, 1)),
                ],
                context.actions
            )
        }
    }

    mod functional_tests {
        use super::*;
        use crate::random;
        use crate::TileState::{Crossed, Filled};

        #[rstest]
        #[case((5, 5), 1000)]
        fn can_solve_some_nxm_boards(#[case] (num_rows, num_cols): (usize, usize), #[case] trials: usize) {
            for _ in 0..trials {
                let board = random::random_board(num_rows, num_cols, 0.5);

                let clues = board.clues();
                let (result, _solved) = blocking_logical_solver(&clues);

                assert_eq!(
                    board,
                    result
                )
            }
        }

        #[rstest]
        #[case( // empty nonogram board
            NonogramBoard::from_matrix(
                Array2::from_shape_vec(
                    (5, 5),
                    vec![Crossed; 25]
                ).unwrap()
            )
        )]
        #[case( // fully filled nonogram board
            NonogramBoard::from_matrix(
                Array2::from_shape_vec(
                    (5, 5),
                    vec![Filled; 25]
                ).unwrap()
            )
        )]
        #[case( // checker board
            NonogramBoard::from_matrix(
                Array2::from_shape_vec(
                    (3, 3),
                    vec![
                        Filled, Crossed, Filled,
                        Crossed, Filled, Crossed,
                        Filled, Crossed, Filled,
                    ]
                ).unwrap()
            )
        )]
        #[case( // smiley board
            NonogramBoard::from_matrix(
                Array2::from_shape_vec(
                    (5, 5),
                    vec![
                        Filled, Filled, Crossed, Filled, Filled,
                        Filled, Filled, Crossed, Filled, Filled,
                        Crossed, Crossed, Crossed, Crossed, Crossed,
                        Filled, Crossed, Crossed, Crossed, Filled,
                        Filled, Filled, Filled, Filled, Filled,
                    ]
                ).unwrap()
            )
        )]
        pub fn solves_board(#[case] board: NonogramBoard) {
            let clues = board.clues();
            let (result, _solved) = blocking_logical_solver(&clues);

            assert_eq!(board, result)
        }

        #[rstest]
        #[case(
            NonogramBoard::from_matrix(
                Array2::from_shape_vec(
                    (4, 4),
                    vec![
                        Filled, Crossed, Filled, Crossed,
                        Crossed, Filled, Crossed, Filled,
                        Filled, Crossed, Filled, Crossed,
                        Crossed, Filled, Crossed, Filled,
                    ]
                ).unwrap()
            )
        )]
        #[case( // smiley board
            NonogramBoard::from_matrix(
                Array2::from_shape_vec(
                    (5, 5),
                    vec![
                        Filled, Filled, Crossed, Filled, Filled,
                        Filled, Filled, Crossed, Filled, Filled,
                        Crossed, Crossed, Crossed, Crossed, Crossed,
                        Filled, Crossed, Crossed, Crossed, Filled,
                        Crossed, Filled, Filled, Filled, Crossed,
                    ]
                ).unwrap()
            )
        )]
        fn cant_solve_board(#[case] board: NonogramBoard) {
            let clues = board.clues();
            let (result, _solved) = blocking_logical_solver(&clues);

            assert_ne!(board, result)
        }

        #[test]
        fn big_board() {
            let board = NonogramBoard::from_matrix(Array2::from_shape_vec(
                (15, 15),
                vec![
                    Crossed, Filled, Crossed, Crossed, Crossed, Filled, Crossed, Crossed, Crossed, Filled, Crossed, Crossed, Crossed, Filled, Filled,
                    Crossed, Crossed, Filled, Crossed, Filled, Crossed, Filled, Filled, Crossed, Crossed, Crossed, Crossed, Crossed, Crossed, Crossed,
                    Filled, Crossed, Crossed, Crossed, Crossed, Filled, Filled, Crossed, Crossed, Filled, Filled, Filled, Crossed, Filled, Crossed,
                    Crossed, Filled, Filled, Crossed, Crossed, Filled, Filled, Filled, Filled, Filled, Filled, Crossed, Crossed, Crossed, Filled,
                    Crossed, Crossed, Crossed, Filled, Crossed, Filled, Filled, Filled, Filled, Filled, Crossed, Filled, Crossed, Filled, Crossed,
                    Crossed, Crossed, Filled, Crossed, Filled, Filled, Filled, Filled, Filled, Filled, Filled, Crossed, Filled, Filled, Filled,
                    Filled, Filled, Crossed, Crossed, Filled, Crossed, Crossed, Crossed, Filled, Crossed, Filled, Filled, Filled, Filled, Filled,
                    Filled, Crossed, Filled, Filled, Crossed, Filled, Crossed, Filled, Crossed, Filled, Crossed, Filled, Crossed, Filled, Filled,
                    Filled, Filled, Crossed, Crossed, Crossed, Crossed, Filled, Filled, Crossed, Crossed, Filled, Crossed, Filled, Filled, Crossed,
                    Filled, Crossed, Crossed, Filled, Filled, Crossed, Filled, Filled, Filled, Filled, Filled, Filled, Crossed, Crossed, Filled,
                    Crossed, Crossed, Filled, Filled, Crossed, Crossed, Crossed, Filled, Crossed, Filled, Crossed, Filled, Filled, Crossed, Crossed,
                    Crossed, Filled, Filled, Filled, Filled, Filled, Filled, Crossed, Filled, Crossed, Filled, Filled, Filled, Crossed, Crossed,
                    Filled, Filled, Crossed, Filled, Crossed, Filled, Crossed, Filled, Filled, Crossed, Crossed, Filled, Filled, Crossed, Filled,
                    Filled, Crossed, Filled, Filled, Crossed, Crossed, Crossed, Filled, Crossed, Filled, Crossed, Filled, Filled, Filled, Filled,
                    Filled, Filled, Filled, Filled, Filled, Filled, Crossed, Crossed, Crossed, Filled, Crossed, Crossed, Filled, Crossed, Filled,
                ]
            ).unwrap());

            let (result, solved) = blocking_logical_solver(&board.clues());

            assert!(solved);
            assert_eq!(result, board);
        }
    }
}