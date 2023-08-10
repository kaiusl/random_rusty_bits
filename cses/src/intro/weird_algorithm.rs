fn solve(mut n: u64) -> Vec<u64> {
    let mut result = Vec::from([n]);
    while n > 1 {
        if n % 2 == 0 {
            // even
            n /= 2;
            result.push(n);
        } else {
            // odd
            n = 3 * n + 1;
            result.push(n);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calc() {
        assert_eq!(solve(0), vec![0]);
        assert_eq!(solve(1), vec![1]);
        assert_eq!(solve(2), vec![2, 1]);
        assert_eq!(solve(3), vec![3, 10, 5, 16, 8, 4, 2, 1]);
        assert_eq!(solve(4), vec![4, 2, 1]);
        assert_eq!(solve(5), vec![5, 16, 8, 4, 2, 1]);
        assert_eq!(solve(6), vec![6, 3, 10, 5, 16, 8, 4, 2, 1]);
        assert_eq!(solve(7), vec![7, 22, 11, 34, 17, 52, 26, 13, 40, 20, 10, 5, 16, 8, 4, 2, 1]);
    }
}
