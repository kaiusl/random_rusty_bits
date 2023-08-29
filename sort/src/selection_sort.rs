use std::mem;

pub fn selection_sort<T>(slice: &mut [T])
where
    T: Ord,
{
    // raw impl with indices
    for i in 0..slice.len() {
        let mut min_index = i;
        let mut min = &slice[i];
        for (j, it) in (i + 1..).zip(&slice[i + 1..]) {
            if it < min {
                min_index = j;
                min = it;
            }
        }

        if min_index != i {
            slice.swap(i, min_index);
        }
    }
}

pub fn selection_sort2<T>(slice: &mut [T])
where
    T: Ord,
{
    // more idiomatic impl
    for i in 0..slice.len() {
        // slice[..i] is sorted, slice[i..] is unsorted
        // all items in sorted are smaller than any item in unsorted
        // find next smallest item in unsorted slice and move it to the end of sorted

        // split at i+1 so that two items we need to swap are in different slices
        // in almost_sorted thus last element it unsorted and we need to find the next smallest to fill it
        let (almost_sorted, unsorted) = slice.split_at_mut(i + 1);
        let first_unsorted = almost_sorted.last_mut().unwrap();

        let min = unsorted.iter_mut().min();
        match min {
            // min <= first_unsorted, no need to swap if it's already the smallest one
            Some(min) if min < first_unsorted => mem::swap(first_unsorted, min),
            _ => {}
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
    //#[cfg_attr(miri, ignore = "no unsafe code, nothing for miri to check")]
    fn selection_sort_test() {
        let mut arr = vec![1, 4, 2, 24, 65, 3, 3, 45];
        selection_sort(arr.as_mut_slice());
        assert_sorted(&arr);
    }

    #[test]
    //#[cfg_attr(miri, ignore = "no unsafe code, nothing for miri to check")]
    fn selection_sort2_test() {
        let mut arr = vec![1, 4, 2, 24, 65, 3, 3, 45];
        selection_sort2(arr.as_mut_slice());
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
            #[cfg_attr(miri, ignore = "no unsafe code, nothing for miri to check")]
            fn selection_sort_test(
                mut vec in proptest::collection::vec(0..10000i32, 0..VEC_SIZE),
            ) {
               selection_sort(vec.as_mut_slice());
               assert_sorted(&vec);
            }

            #[test]
            #[cfg_attr(miri, ignore = "no unsafe code, nothing for miri to check")]
            fn selection_sort2_test(
                mut vec in proptest::collection::vec(0..10000i32, 0..VEC_SIZE),
            ) {
               selection_sort2(vec.as_mut_slice());
               assert_sorted(&vec);
            }
        );
    }
}
