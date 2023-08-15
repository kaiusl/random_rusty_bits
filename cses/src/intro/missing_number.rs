fn solve(count: u64, mut numbers: Vec<u64>) -> u64 {
    assert!(
        numbers.len() as u64 == count - 1,
        "expected `count-1`={} numbers, got {}",
        count - 1,
        numbers.len()
    );

    numbers.sort();
    assert!(
        numbers.is_empty() || numbers.last().unwrap() <= &count,
        "expected consecutive numbers `1, 2 ... count` except one, found at least one larger number {}",
        numbers.last().unwrap()
    );

    match numbers.as_slice() {
        // Count == 1 because of the assert above
        [] => count,
        // Special case the first and last
        [first, ..] if *first != 1 => 1,
        [.., last] if *last != count => count,
        // We are missing some middle digit
        numbers => {
            numbers
                .windows(2)
                .map(|lr| {
                    let l = lr[0];
                    let r = lr[1];

                    l.abs_diff(r)
                })
                .position(|diff| diff != 1)
                .unwrap() as u64
                + 2
            // +2 because indices start at 0 but our numbers start at 1
            // and windows takes two items at once, so the diff == 2 is shifted
            // by 1 more position
            // For example:
            // numbers: 1 2 3 5 -> iter: 1, 1, 2 -> position: 2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        assert_eq!(solve(5, vec![2, 3, 1, 5]), 4);
        assert_eq!(solve(5, vec![2, 3, 1, 4]), 5);
        assert_eq!(solve(5, vec![2, 3, 5, 4]), 1);
        assert_eq!(solve(10, vec![2, 3, 7, 8, 5, 4, 9, 10, 6]), 1);
        assert_eq!(solve(1, vec![]), 1);
    }

    #[test]
    #[should_panic]
    fn test2() {
        assert_eq!(solve(10, vec![2, 3, 7, 8, 5, 4, 10, 6]), 1);
    }

    #[test]
    #[should_panic]
    fn test3() {
        assert_eq!(solve(10, vec![2, 3, 7, 8, 5, 4, 11, 9, 6]), 1);
    }
}
