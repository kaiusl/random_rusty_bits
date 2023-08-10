fn solve(n: u64) -> u64 {
    let total_positions = (n*n-1..=n*n).product::<u64>()/2;
    total_positions - (4*(n-1)*(n-2))
}


// a b a b a
// b a a a b
// a a c a a
// b a a a b
// a b a b a

// Middle positions (remove 2 from each edge) rules out 8 positions.
// Corners rule out 2 positions
// Adjacent to corner on the edge rules out 3 positions
// Adjacent to corner to the inside rules out 4 positions
// Middle edge rules out 4 positions
// Inner edge rules out 6 positions
// 



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() {
        assert_eq!(solve(2), 6);
        assert_eq!(solve(3), 28);
        assert_eq!(solve(4), 96);
        assert_eq!(solve(7), 1056);
    }
}