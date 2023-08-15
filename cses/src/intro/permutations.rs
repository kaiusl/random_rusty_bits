fn solve(n: u64) -> Option<Vec<u64>> {
    if n <= 3 {
        return None;
    }

    let mut result = Vec::with_capacity(n as usize);

    result.extend((2..=n).step_by(2));
    result.extend((1..=n).step_by(2));

    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_diff(slice: &[u64]) {
        for pair in slice.windows(2) {
            assert!(pair[0].abs_diff(pair[1]) > 1);
        }
    }

    #[test]
    fn test() {
        assert_eq!(solve(3), None);
        assert_eq!(solve(4), Some(vec![2, 4, 1, 3]));
        check_diff(&solve(4).unwrap());
        assert_eq!(solve(5), Some(vec![2, 4, 1, 3, 5]));
        check_diff(&solve(5).unwrap());
        assert_eq!(solve(6), Some(vec![2, 4, 6, 1, 3, 5]));
        check_diff(&solve(6).unwrap());
    }
}
