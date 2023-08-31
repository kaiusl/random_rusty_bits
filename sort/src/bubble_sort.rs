pub fn bubble_sort<T: Ord>(slice: &mut [T]) {
    for iteration in 0..slice.len() {
        let mut is_sorted = true;
        for i in 0..slice.len() - 1 - iteration {
            if slice[i] > slice[i + 1] {
                slice.swap(i, i + 1);
                is_sorted = false;
            }
        }
        if is_sorted {
            break;
        }
    }
}

pub fn bubble_sort2<T: Ord>(mut slice: &mut [T]) {
    if slice.len() < 2 {
        return;
    }

    while slice.len() > 1 {
        let mut new_unsorted_len = 0;
        for i in 0..slice.len() - 1 {
            let j = i + 1;
            if slice[i] > slice[j] {
                slice.swap(i, j);
                new_unsorted_len = j;
            }
        }
        slice = &mut slice[..new_unsorted_len];
    }
}

pub fn bubble_sort2_unsafe<T: Ord>(slice: &mut [T]) {
    if slice.len() < 2 {
        return;
    }

    let mut unsorted_len = slice.len();
    let ptr = slice.as_mut_ptr();
    unsafe {
        // SAFETY: unsorted_len <= slice.len() always, thus
        // `i < unsorted_len - 1 <= slice.len() - 1` and `j = i + 1 < unsorted_len <= slice.len()`
        // in short: `i` and `j` are in bounds of slice
        while unsorted_len > 1 {
            let mut new_unsorted_len = 0;
            for i in 0..unsorted_len - 1 {
                let j = i + 1;
                let i_ptr = ptr.add(i);
                let j_ptr = ptr.add(j);
                if *i_ptr > *j_ptr {
                    core::ptr::swap(i_ptr, j_ptr);
                    new_unsorted_len = j + 1;
                };
            }
            unsorted_len = new_unsorted_len
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
    #[cfg_attr(miri, ignore = "no unsafe code, nothing for miri to check")]
    fn test() {
        let mut arr = vec![1, 4, 2, 24, 65, 3, 3, 45];
        bubble_sort(arr.as_mut_slice());
        assert_sorted(&arr);
    }

    #[test]
    #[cfg_attr(miri, ignore = "no unsafe code, nothing for miri to check")]
    fn test2() {
        let mut arr = vec![1, 4, 2, 24, 65, 3, 3, 45];
        bubble_sort2(arr.as_mut_slice());
        assert_sorted(&arr);
    }

    #[test]
    fn test2_unsafe() {
        let mut arr = vec![1, 4, 2, 24, 65, 3, 3, 45];
        bubble_sort2_unsafe(arr.as_mut_slice());
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
               bubble_sort(vec.as_mut_slice());
               assert_sorted(&vec);
            }

            #[test]
            #[cfg_attr(miri, ignore = "no unsafe code, nothing for miri to check")]
            fn test2(
                mut vec in proptest::collection::vec(0..10000i32, 0..VEC_SIZE),
            ) {
               bubble_sort2(vec.as_mut_slice());
               assert_sorted(&vec);
            }

            #[test]
            fn test2_unsafe(
                mut vec in proptest::collection::vec(0..10000i32, 0..VEC_SIZE),
            ) {
               bubble_sort2_unsafe(vec.as_mut_slice());
               assert_sorted(&vec);
            }
        );
    }
}
