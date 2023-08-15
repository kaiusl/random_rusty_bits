fn solve(nums: Vec<u64>) -> u64 {
    if nums.len() < 2 {
        return 0;
    }

    nums.windows(2)
        .map(|a| {
            let l = a[0];
            let r = a[1];

            l.saturating_sub(r)
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        assert_eq!(solve(vec![3, 2, 5, 1, 7]), 5);
        assert_eq!(solve(vec![1, 2, 3, 4, 5]), 0);
        assert_eq!(solve(vec![1, 1, 1, 1, 1]), 0);
    }
}
