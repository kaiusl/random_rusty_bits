fn solve(n: usize) -> Option<(Vec<usize>, Vec<usize>)> {
    let sum = (1..=n).sum::<usize>();
    if is_odd(sum) {
        None
    } else {
        let size = n / 2 + 1;
        // Only even sums are divisible into two
        let mut set_a = Vec::with_capacity(size);
        let mut set_b = Vec::with_capacity(size);
        let mut nums = 1..=n;

        // If n is odd, we need some set of numbers where a + b = c.
        // Luckily we always have such numbers as 1 + 2 = 3.
        //
        // After distributing these numbers we must have even number of numbers left
        // and in such cases we know that first + last = second + second to last.
        // Note that the number of numbers left must also be divisible by 4 because
        // otherwise the sum cannot be even.
        // In other words the only values of n that are separable into two sets
        // with equals sums are if n or n+1 is divisible by 4.

        if is_odd(n) {
            set_a.push(nums.next().unwrap());
            set_a.push(nums.next().unwrap());
            set_b.push(nums.next().unwrap());
        }

        assert!(nums.clone().count() % 4 == 0);

        while let (Some(front), Some(back)) = (nums.next(), nums.next_back()) {
            set_a.push(front);
            set_a.push(back);
            set_b.push(nums.next().unwrap());
            set_b.push(nums.next_back().unwrap());
        }

        Some((set_a, set_b))
    }
}

#[inline(always)]
fn is_even(n: usize) -> bool {
    n & 1 == 0
}

#[inline(always)]
fn is_odd(n: usize) -> bool {
    !is_even(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_eq_sums(a: &[usize], b: &[usize]) {
        assert_eq!(a.iter().sum::<usize>(), b.iter().sum::<usize>());
    }

    #[test]
    fn test() {
        assert_eq!(solve(6), None);
        assert_eq!(solve(3), Some((vec![1, 2], vec![3])));
        assert_eq!(solve(7), Some((vec![1, 2, 4, 7], vec![3, 5, 6])));
        assert_eq!(solve(8), Some((vec![1, 8, 3, 6], vec![2, 7, 4, 5])));
        let Some((a, b)) = solve(199999) else {
            panic!()
        };
        test_eq_sums(&a, &b);
    }

    #[test]
    fn test2() {
        for n in 0..10000 {
            if n % 4 == 0 || (n + 1) % 4 == 0 {
                let Some((a, b)) = solve(n) else { panic!() };
                test_eq_sums(&a, &b);
            } else {
                assert!(solve(n).is_none());
            }
        }
    }
}
