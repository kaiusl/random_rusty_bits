pub fn insertion_sort<T>(slice: &mut [T])
where
    T: Ord,
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
    T: Ord,
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
    #[cfg_attr(miri, ignore = "no unsafe code, nothing for miri to check")]
    fn test() {
        let mut arr = vec![1, 4, 2, 24, 65, 3, 3, 45];
        insertion_sort(arr.as_mut_slice());
        assert_sorted(&arr);
    }

    #[test]
    #[cfg_attr(miri, ignore = "no unsafe code, nothing for miri to check")]
    fn test2() {
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
            #[cfg_attr(miri, ignore = "no unsafe code, nothing for miri to check")]
            fn test(
                mut vec in proptest::collection::vec(0..10000i32, 0..VEC_SIZE),
            ) {
               insertion_sort(vec.as_mut_slice());
               assert_sorted(&vec);
            }

            #[test]
            #[cfg_attr(miri, ignore = "no unsafe code, nothing for miri to check")]
            fn test2(
                mut vec in proptest::collection::vec(0..10000i32, 0..VEC_SIZE),
            ) {
               insertion_sort2(vec.as_mut_slice());
               assert_sorted(&vec);
            }
        );
    }
}
