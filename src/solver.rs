use std::cmp;
use crate::{BoardIdx, LineDir, LineRange, NonogramBoard, NonogramClues, NonogramLine};
use std::ops::{Add, Range};
use rand::fill;

type ClueIdx = usize;

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
    fn single_range(&self) -> Option<&Range<usize>> {
        match self {
            ClueSpan::Filled { span, .. } | ClueSpan::Single(span) => Some(span),
            ClueSpan::Multi(_) => None,
        }
    }

    fn overlapped(&self, range: Range<usize>) -> Self {
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

                ClueSpan::Filled { filled_span, span: new_range }
            },
            ClueSpan::Single(span) => {
                let start = cmp::max(span.start, range.start);
                let end = cmp::min(span.end, range.end);

                ClueSpan::Single(start..end)
            }
            ClueSpan::Multi(spans) => {
                ClueSpan::Multi(
                    spans.clone().into_iter().map(|span_range| {
                        let start = cmp::max(span_range.start, range.start);
                        let end = cmp::min(span_range.end, range.end);
                        start..end
                    }).collect()
                )
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
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn span(&self) -> &ClueSpan {
        &self.span
    }
}

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
}
// TODO ON RETURN generate working clues from nonogram board

#[derive(PartialEq, Eq, Debug, Clone)]
enum SolverAction {
    UpdateClueSpan(ClueSpan, ClueIdx),
    FillTile(BoardIdx, ClueIdx),
    FillRange(LineRange, ClueIdx),
    CrossTile(BoardIdx, ClueIdx),
    CrossRange(LineRange, ClueIdx),
}
struct SolverCtx {
    line_dir: LineDir,
    line_idx: usize,
    actions: Vec<SolverAction>,
}
impl SolverCtx {
    fn update_clue_span(&mut self, new_span: ClueSpan, clue_idx: ClueIdx) {
        self.actions.push(SolverAction::UpdateClueSpan(new_span, clue_idx))
    }
    fn fill_tile(&mut self, idx: usize, clue_idx: ClueIdx) {
        let tile = BoardIdx::directional(self.line_dir, idx, self.line_idx);
        let action = SolverAction::FillTile(tile, clue_idx);
        self.actions.push(action);
    }
    fn fill_range(&mut self, range: Range<usize>, clue_idx: ClueIdx) {
        let tile = LineRange::new(self.line_dir, self.line_idx, range);
        let action = SolverAction::FillRange(tile, clue_idx);
        self.actions.push(action);
    }
}

fn overlap_pass(ctx: &mut SolverCtx, clues: &Vec<Clue>) {
    clues.iter().enumerate()
        .for_each(|(clue_idx, Clue{ len, span })| {

            let overlap_range = match span {
                ClueSpan::Filled { span, .. } | ClueSpan::Single(span) => {
                    let start = span.end - len;
                    let end = span.start + len;

                    Some(start..end)
                },
                ClueSpan::Multi(_) => { None }
            };

            overlap_range.inspect(|range|
                ctx.fill_range(range.clone(), clue_idx)
            );
        });
}
fn overlap_with_most_liberal_spans(ctx: &mut SolverCtx, line_len: usize, clues: &Vec<Clue>) {
    let clue_spaces_sum: usize = clues.iter()
        .map(|clue| clue.len())
        .sum::<usize>()
        .add(clues.len() - 1);

    clues.iter()
        .scan(
            0_usize,
            |clue_spaces_before, clue| {
                let clue_spaces_sum_after = clue_spaces_sum - *clue_spaces_before - clue.len();
                let liberal_span_range = *clue_spaces_before..(line_len - clue_spaces_sum_after);

                *clue_spaces_before += clue.len() + 1;
                return Some((liberal_span_range, clue))
            }
        )
        .enumerate()
        .map(|(clue_idx, (liberal_span_range, clue))| (liberal_span_range, clue, clue_idx))
        .filter(|(liberal_span_range, clue, _clue_idx)| {
            !clue.span.single_range().is_some_and(|range| {
                range.start >= liberal_span_range.start && range.end <= liberal_span_range.end
            })
        })
        .for_each(|(liberal_span_range, clue, clue_idx)| {
            let new_span = clue.span
                .overlapped(liberal_span_range)
                .trim_spans_below_len(clue.len);

            ctx.update_clue_span(new_span, clue_idx)
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NonogramBoard, TileState};
    use ndarray::Array2;
    use TileState::Filled;

    #[test]
    fn test_complete_overlap() {
        let clues = vec![
            Clue{ len: 5, span: ClueSpan::Single(0..5) }
        ];

        let mut context = SolverCtx {
            line_dir: LineDir::Row,
            line_idx: 0,
            actions: vec![],
        };

        overlap_pass(&mut context, &clues);

        assert_eq!(
            vec![
                SolverAction::FillRange(
                    LineRange::new(LineDir::Row, 0, 0..5),
                    0
                ),
            ],
            context.actions
        )
    }
    #[test]
    fn test_partial_overlap() {
        let clues = vec![
            Clue{ len: 3, span: ClueSpan::Single(0..5) }
        ];

        let mut context = SolverCtx {
            line_dir: LineDir::Row,
            line_idx: 0,
            actions: vec![],
        };

        overlap_pass(&mut context, &clues);

        assert_eq!(
            vec![
                SolverAction::FillRange(
                    LineRange::new(LineDir::Row, 0, 2..3),
                    0
                ),
            ],
            context.actions
        )
    }
    #[test]
    fn test_overlap_with_most_liberal_spans() {
        let clues = vec![
            Clue{ len: 2, span: ClueSpan::Single(0..5) },
            Clue{ len: 2, span: ClueSpan::Single(0..5) },
        ];

        let mut context = SolverCtx {
            line_dir: LineDir::Row,
            line_idx: 0,
            actions: vec![],
        };

        overlap_with_most_liberal_spans(&mut context, 5, &clues);

        assert_eq!(
            vec![
                SolverAction::UpdateClueSpan(
                    ClueSpan::Single(Range::from(0..2)),
                    0
                ),
                SolverAction::UpdateClueSpan(
                    ClueSpan::Single(Range::from(3..5)),
                    1
                ),
            ],
            context.actions
        )
    }
    #[test]
    fn test_overlap_with_most_liberal_spans_when_multi_spans_are_involved() {
        let clues = vec![
            Clue{ len: 1, span: ClueSpan::Multi(vec![0..1, 2..3]) },
            Clue{ len: 1, span: ClueSpan::Multi(vec![0..1, 2..3]) },
        ];

        let mut context = SolverCtx {
            line_dir: LineDir::Row,
            line_idx: 0,
            actions: vec![],
        };

        overlap_with_most_liberal_spans(&mut context, 3, &clues);

        assert_eq!(
            vec![
                SolverAction::UpdateClueSpan(
                    ClueSpan::Single(Range::from(0..1)),
                    0
                ),
                SolverAction::UpdateClueSpan(
                    ClueSpan::Single(Range::from(2..3)),
                    1
                ),
            ],
            context.actions
        )
    }
    #[test]
    fn test_overlap_with_most_liberal_spans_with_a_mix_of_spans() {
        // aaBaCCC
        // aa
        //   BB
        //     CCC

        let clues = vec![
            Clue{ len: 1, span: ClueSpan::Multi(vec![0..2, 3..4]) },
            Clue{ len: 1, span: ClueSpan::Filled{ filled_span: 2..3, span: 2..3 } },
            Clue{ len: 2, span: ClueSpan::Filled { filled_span: 5..6, span: 4..7 } },
        ];

        let mut context = SolverCtx {
            line_dir: LineDir::Row,
            line_idx: 0,
            actions: vec![],
        };

        overlap_with_most_liberal_spans(&mut context, 7, &clues);

        assert_eq!(
            vec![
                SolverAction::UpdateClueSpan(
                    ClueSpan::Single(Range::from(0..2)), // Note: this is unintuitively 0..2 NOT 0..1
                    0
                ),
                // no update for B
                // no update for C
            ],
            context.actions
        )
    }
}