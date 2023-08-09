#![allow(dead_code)]
#![deny(rust_2018_idioms)]
#![deny(unsafe_op_in_unsafe_fn)]

pub fn linear_search(slice: &[i32], needle: i32) -> Option<usize> {
    for (i, it) in slice.iter().enumerate() {
        if *it == needle {
            return Some(i);
        }
    }

    None
}

pub fn binary_search(slice: &[i32], needle: i32) -> Option<usize> {
    if slice.is_empty() {
        return None;
    }

    let mut l = 0;
    let mut r = slice.len();
    let mut mid = r / 2;

    while l < r {
        match needle.cmp(&slice[mid]) {
            core::cmp::Ordering::Less => r = mid,
            core::cmp::Ordering::Equal => return Some(mid),
            core::cmp::Ordering::Greater => l = mid + 1,
        }

        mid = l + (r - l) / 2;
    }

    None
}

/// Jump search with jump size sqrt(n).
///
/// Time complexity of O(sqrt(n)) since we are doing a maximum of sqrt(n) jumps
/// + maximum of sqrt(n) steps in linear search
pub fn jump_search(slice: &[i32], needle: i32) -> Option<usize> {
    if slice.is_empty() {
        return None;
    }

    let size = slice.len();
    let jump_size = f64::sqrt(size as f64) as usize;
    let mut l = 0;

    while l < size {
        let mid = l + jump_size;
        match needle.cmp(&slice[mid]) {
            core::cmp::Ordering::Less => return linear_search(&slice[l..], needle),
            core::cmp::Ordering::Equal => return Some(mid),
            core::cmp::Ordering::Greater => {}
        }
        l = mid;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let v = vec![1, 2, 3, 5, 7, 8, 9];
        assert_eq!(jump_search(&v, 1), Some(0));
        assert_eq!(jump_search(&v, 3), Some(2));
        assert_eq!(jump_search(&v, 9), Some(6));
    }
}
