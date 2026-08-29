//! Adaptive grid layout math shared by the interactive picker, `exc list`,
//! and the sysinfo box. Given a terminal width and the widest cell content,
//! decide how many columns fit; callers turn a flat item count into a
//! column-major grid (numbered top-to-bottom within a column, then across to
//! the next column — the same fill order as `ls` multi-column output and
//! a shell `select` menu) using the returned column count.

use unicode_width::UnicodeWidthStr;

/// Extra horizontal space reserved per column for gutter/padding between cells.
pub const COLUMN_GUTTER: usize = 2;

/// Compute how many columns fit in `available_width`, given the widest cell
/// is `cell_width` columns wide. Always returns at least 1.
pub fn grid_columns(available_width: usize, cell_width: usize, item_count: usize) -> usize {
    if item_count == 0 {
        return 1;
    }
    let col_width = cell_width + COLUMN_GUTTER;
    let columns = available_width.checked_div(col_width).unwrap_or(item_count).max(1);
    columns.min(item_count)
}

/// Column-major grid position for `index`, given the grid is `rows` tall.
/// Items fill top-to-bottom within a column before moving to the next one.
pub fn grid_position(index: usize, rows: usize) -> (usize, usize) {
    let rows = rows.max(1);
    (index % rows, index / rows)
}

/// Index into a flat item list from a column-major grid position (`rows`
/// tall per column).
pub fn index_from_position(row: usize, col: usize, rows: usize) -> usize {
    col * rows.max(1) + row
}

/// Display width of a string, unicode-aware.
pub fn display_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

/// Number of rows needed to lay out `item_count` items across `columns`.
pub fn rows_needed(item_count: usize, columns: usize) -> usize {
    if item_count == 0 {
        return 0;
    }
    item_count.div_ceil(columns.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_never_zero() {
        assert_eq!(grid_columns(0, 10, 5), 1);
        assert_eq!(grid_columns(80, 0, 5), 5);
    }

    #[test]
    fn columns_fit_width() {
        // width 40, cell 10 + gutter 2 = 12 per column -> 3 columns fit
        assert_eq!(grid_columns(40, 10, 100), 3);
    }

    #[test]
    fn columns_capped_by_item_count() {
        assert_eq!(grid_columns(200, 5, 2), 2);
    }

    #[test]
    fn position_round_trips() {
        let rows = 4;
        for index in 0..17 {
            let (row, col) = grid_position(index, rows);
            assert_eq!(index_from_position(row, col, rows), index);
        }
    }

    #[test]
    fn fills_top_to_bottom_within_a_column_before_the_next_column() {
        // 7 items, 3 rows tall -> columns of [0,1,2], [3,4,5], [6]
        let rows = 3;
        assert_eq!(grid_position(0, rows), (0, 0));
        assert_eq!(grid_position(1, rows), (1, 0));
        assert_eq!(grid_position(2, rows), (2, 0));
        assert_eq!(grid_position(3, rows), (0, 1));
        assert_eq!(grid_position(4, rows), (1, 1));
        assert_eq!(grid_position(6, rows), (0, 2));
    }

    #[test]
    fn rows_needed_ceils() {
        assert_eq!(rows_needed(10, 3), 4);
        assert_eq!(rows_needed(9, 3), 3);
        assert_eq!(rows_needed(0, 3), 0);
    }
}
