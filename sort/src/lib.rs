#![allow(dead_code)]
#![deny(rust_2018_idioms)]
#![deny(unsafe_op_in_unsafe_fn)]

pub mod merge_sort;

pub fn bubble_sort<T: PartialOrd>(slice: &mut [T]) {
    for iteration in 0..slice.len() {
        for i in 0..slice.len() - 1 - iteration {
            if slice[i] > slice[i + 1] {
                slice.swap(i, i + 1);
            }
        }
    }
}

pub fn insertion_sort<T>(slice: &mut [T])
where
    T: PartialOrd,
{
    for j in 1..slice.len() {
        let to_sort = &slice[j];
        let mut new_index = 0;
        for i in (0..j).rev() {
            if &slice[i] < to_sort {
                new_index = i + 1;
                break;
            }
        }
        slice[new_index..=j].rotate_right(1);
    }
}

pub fn insertion_sort2<T>(slice: &mut [T])
where
    T: PartialOrd,
{
    for j in 1..slice.len() {
        let to_sort = &slice[j];
        let new_index = slice[..j].partition_point(|a| a < to_sort);
        slice[new_index..=j].rotate_right(1);
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

    #[test]
    fn insertion_sort_test() {
        let mut arr = vec![1, 4, 2, 24, 65, 3, 3, 45];
        insertion_sort(arr.as_mut_slice());
        assert_sorted(&arr);
    }

    #[test]
    fn insertion_sort2_test() {
        let mut arr = vec![1, 4, 2, 24, 65, 3, 3, 45];
        insertion_sort2(arr.as_mut_slice());
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

            #[test]
            fn insertion_sort_test(
                mut vec in proptest::collection::vec(0..10000i32, 0..VEC_SIZE),
            ) {
               insertion_sort(vec.as_mut_slice());
               assert_sorted(&vec);
            }

            #[test]
            fn insertion_sort2_test(
                mut vec in proptest::collection::vec(0..10000i32, 0..VEC_SIZE),
            ) {
               insertion_sort2(vec.as_mut_slice());
               assert_sorted(&vec);
            }
        );
    }
}
