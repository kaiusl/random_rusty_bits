fn solve(row: usize, col: usize) -> usize {
    //  1  2  9 10
    //  4  3  8 11
    //  5  6  7 12
    // 16 15 14 13

    // IDEA:
    //   We can calculate the number in top row for each layer and then
    //   from the coordinates we can calculate how many steps from the top row we
    //   must take to reach our desired position. If the row is increasing we must
    //   add to the top row value and if it's decreasing we must subtract from the
    //   top row value.
    //
    //   Even layers are increasing and odd layers are decreasing from the top row.
    //   We can calculate the digit in the top row as the number of numbers in
    //   previous layers for even/increasing layers or as the number of numbers
    //   including the current layer for odd/decreasing layers.

    let layer = usize::max(col, row);
    let is_increasing = layer % 2 == 0;
    let steps = (layer - col) + (row - 1);

    if is_increasing {
        let prev_layer = layer - 1;
        let top = prev_layer * prev_layer + 1;
        top + steps
    } else {
        let top = layer * layer;
        top - steps
    }
}

fn construct(n: usize) -> Vec<Vec<usize>> {
    let mut columns = Vec::with_capacity(n);
    for _ in 0..n {
        columns.push(Vec::with_capacity(n));
    }

    columns[0].push(1);

    let mut layer = 1;

    while layer < n {
        let mut col = layer;

        if layer % 2 == 0 {
            let mut val = (layer + 1) * (layer + 1);
            for _ in 0..layer {
                columns[col].push(val);
                val -= 1;
            }

            columns[col].push(val);
            val -= 1;

            for _ in 0..layer {
                col -= 1;
                columns[col].push(val);
                val -= 1;
            }
        } else {
            let mut val = layer * layer + 1;
            for _ in 0..layer {
                columns[col].push(val);
                val += 1;
            }

            columns[col].push(val);
            val += 1;

            for _ in 0..layer {
                col -= 1;
                columns[col].push(val);
                val += 1;
            }
        }

        layer += 1;
    }

    columns
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        assert_eq!(solve(2, 3), 8);
        assert_eq!(solve(1, 1), 1);
        assert_eq!(solve(4, 2), 15);

        // let n = 4;
        // for row in 0..n {
        //     for col in 0..n {
        //         print!("{}, ", calc(row + 1, col + 1));
        //     }
        //     print!("\n")
        // }
    }

    #[test]
    fn test_construct() {
        let n = 5;
        let mat = construct(n);
        println!("{mat:#?}");

        // for i in 0..n {
        //     for j in 0..n {
        //         print!(mat[i][j])
        //     }
        // }
    }
}
