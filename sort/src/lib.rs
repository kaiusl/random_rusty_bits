#![allow(dead_code)]
#![deny(rust_2018_idioms)]
#![deny(unsafe_op_in_unsafe_fn)]

pub fn bubble_sort<T: PartialOrd>(slice: &mut [T]) {
    for iteration in 0..slice.len() {
        for i in 0..slice.len() - 1 - iteration {
            if slice[i] > slice[i + 1] {
                slice.swap(i, i + 1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_sorted(slice: &[i32]) {
        slice.windows(2).for_each(|arr| {
            let a = arr[0];
            let b = arr[1];
            assert!(a <= b);
        })
    }

    #[test]
    fn bubble_sort_test() {
        let mut arr = vec![1, 4, 2, 24, 65, 3, 3, 45];
        bubble_sort(arr.as_mut_slice());
        assert_sorted(&arr);
    }

    mod proptests {
        use proptest::prelude::*;

        use super::*;

        #[cfg(not(miri))]
        const VEC_SIZE: usize = 1000;
        #[cfg(miri)]
        const VEC_SIZE: usize = 50;

        #[cfg(not(miri))]
        const PROPTEST_CASES: u32 = 1000;
        #[cfg(miri)]
        const PROPTEST_CASES: u32 = 10;

        proptest!(
            #![proptest_config(ProptestConfig::with_cases(PROPTEST_CASES))]

            #[test]
            fn bubble_sort_test(
                mut vec in proptest::collection::vec(0..10000i32, 0..VEC_SIZE),
            ) {
               bubble_sort(vec.as_mut_slice());
               assert_sorted(&vec);
            }
        );
    }
}
