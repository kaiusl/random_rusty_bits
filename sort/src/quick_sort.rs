use core::mem;

fn quick_sort_lomuto<T: Ord>(slice: &mut [T]) {
    if slice.len() < 2 {
        return;
    }

    let (l, r) = partition_lomuto(slice);
    if l.len() > 1 {
        quick_sort_lomuto(l);
    }
    if r.len() > 1 {
        quick_sort_lomuto(r);
    }
}

/// Partition the slice around the value of last item in-place using Lomuto's scheme.
///
/// Returns two slices, where first contains items smaller than or equal the last and
/// second items larger than the last. The last item (the pivot) itself is not part of the
/// returned slices, but it's placed in correct sorted position between the returned slices.
///
/// # Panics
///
/// * if `slice` is empty
fn partition_lomuto<T: Ord>(slice: &mut [T]) -> (&mut [T], &mut [T]) {
    // Move every item thats smaller than pivot to left.

    // See https://www.geeksforgeeks.org/quick-sort/ for good illustration on the algorithm
    let (pivot, rest) = slice.split_last_mut().unwrap();

    let mut count_smaller_than_pivot = 0;
    for i in 0..rest.len() {
        if &rest[i] <= pivot {
            if i != count_smaller_than_pivot {
                rest.swap(count_smaller_than_pivot, i);
            }
            count_smaller_than_pivot += 1;
        }
    }

    if count_smaller_than_pivot != rest.len() {
        mem::swap(pivot, &mut rest[count_smaller_than_pivot]);
    } else {
        // pivot was the largest item, it's already at correct location
    }

    let (a, b) = slice.split_at_mut(count_smaller_than_pivot);
    // exclude pivot from the returned slices
    (a, &mut b[1..])
}

fn quick_sort_hoare<T: Ord>(slice: &mut [T]) {
    if slice.len() < 2 {
        return;
    }

    let (l, r) = partition_hoare(slice);
    if l.len() > 1 {
        quick_sort_hoare(l);
    }
    if r.len() > 1 {
        quick_sort_hoare(r);
    }
}

/// Partition the slice around the value of first item in-place using Hoare's scheme.
///
/// Returns two slices, where first contains items smaller than or equal the last and
/// second items larger than the last. The first item (the pivot) itself is not part of the
/// returned slices, but it's placed in correct sorted position between the returned slices.
///
/// # Panics
///
/// * if `slice` is empty
fn partition_hoare<T: Ord>(slice: &mut [T]) -> (&mut [T], &mut [T]) {
    // Overall idea here is to look for smaller items on the right and larger
    // items on the left and swap them. We do that by looking first from the
    // back/right for the smaller items than pivot and then from the left for
    // the larger items. If the two halves meet, then all the items must be
    // partitioned by the pivot. Final step is to move the pivot itselt to the
    // correct position.

    let (pivot, rest) = slice.split_first_mut().unwrap();

    let mut left = 0;
    let mut right = rest.len() - 1;

    while &rest[right] > pivot {
        if right == 0 {
            // all items on the right are already larger than pivot
            return (&mut [], &mut slice[1..]);
        }
        right -= 1;
    }

    // If left == right, then right point
    while left < right {
        debug_assert!(&rest[right] <= pivot);
        debug_assert!(right != 0);
        // Invariants:
        //  `rest[..left]` is `<= pivot`
        //   `rest[right+1..]` is `> pivot`
        //   `rest[right] <= pivot`
        //
        // Termination:
        //   if `left == right` then `rest[..=left] = rest[..=right]` are all `<= pivot`
        //   and `rest[right+1]` are `> pivot`
        //   and we have partitioned tha slice

        // find next item that's larger than `pivot`
        if &rest[left] <= pivot {
            // left is on the correct side
            left += 1
        } else {
            // left > pivot, need to be moved
            rest.swap(left, right);
            // now `rest[right..]` is `> pivot`
            // `rest[..=left]` is `<= pivot`
            // look for the next smaller than `pivot` from the back
            while &rest[right] > pivot {
                right -= 1;
            }
        }
    }

    // now `slice[..=right]` are `<= pivot`, `slice[right+1..]` are `> pivot`

    // swap `pivot` to correct position, `right` points to the last item that's `<= pivot`
    // swap with it so that left to `pivot` is `<= pivot` and right to pivot is `> pivot`
    debug_assert!(&rest[right] <= pivot);
    mem::swap(pivot, &mut rest[right]);
    let (a, b) = slice.split_at_mut(right + 1);
    // exclude `pivot` from the returned slices
    (a, &mut b[1..])
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
    fn test_lomuto() {
        let mut arr = vec![1, 4, 2, 24, 65, 3, 3, 45];
        quick_sort_lomuto(&mut arr);
        assert_sorted(&arr);
    }

    #[test]
    #[cfg_attr(miri, ignore = "no unsafe code, nothing for miri to check")]
    fn test_hoare() {
        let mut arr = vec![1, 4, 2, 24, 65, 3, 3, 45];
        quick_sort_hoare(&mut arr);
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
            fn test_lomuto(
                mut vec in proptest::collection::vec(0..10000i32, 0..VEC_SIZE),
            ) {
               quick_sort_lomuto(vec.as_mut_slice());
               assert_sorted(&vec);
            }

            #[test]
            #[cfg_attr(miri, ignore = "no unsafe code, nothing for miri to check")]
            fn test_hoare(
                mut vec in proptest::collection::vec(0..10000i32, 0..VEC_SIZE),
            ) {
               quick_sort_hoare(vec.as_mut_slice());
               assert_sorted(&vec);
            }

        );
    }
}
