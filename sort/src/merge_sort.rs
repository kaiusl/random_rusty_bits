use core::mem::{self, MaybeUninit};

/// Merge sort that works with only `Copy` types
pub fn merge_sort_copy<T: Ord + Copy>(slice: &mut [T]) {
    let mut tmp = Vec::with_capacity(slice.len());
    tmp.extend(slice.iter().copied());
    merge_sort_copy_core(slice, &mut tmp);
}

/// As a result all items in output are sorted.
fn merge_sort_copy_core<T: Ord>(output: &mut [T], tmp: &mut [T]) {
    if output.len() > 1 {
        let mid = output.len() / 2;
        let (l, r) = output.split_at_mut(mid);
        let (tmpl, tmpr) = tmp.split_at_mut(mid);

        // sort into temporary arrays
        merge_sort_copy_core(tmpl, l);
        merge_sort_copy_core(tmpr, r);
        // merge into actual array we want to sort
        merge_copy(output, tmpl, tmpr);
    } else {
        // single item, must be sorted
    }
}

/// Merge sorted slices l and r into output.
///
/// Note that following must hold: `l.len() + r.len() == output.len()`
fn merge_copy<T: Ord>(output: &mut [T], l: &mut [T], r: &mut [T]) {
    debug_assert_eq!(l.len() + r.len(), output.len());
    let mut l_iter = l.iter_mut();
    let mut r_iter = r.iter_mut();

    let mut l_head = l_iter.next();
    let mut r_head = r_iter.next();
    // take items from left and right one at the time
    // put the smaller of lhead and rhead as the next item in slice
    for it in output.iter_mut() {
        match (&mut l_head, &mut r_head) {
            (None, None) => unreachable!(),
            (None, Some(r)) => {
                mem::swap(it, r);
                r_head = r_iter.next();
            }
            (Some(l), None) => {
                mem::swap(it, l);
                l_head = l_iter.next();
            }
            (Some(l), Some(r)) => {
                if l <= r {
                    mem::swap(it, l);
                    l_head = l_iter.next();
                } else {
                    mem::swap(it, r);
                    r_head = r_iter.next();
                }
            }
        }
    }
}

/// Generic merge sort that also works with non-`Copy` types.
pub fn merge_sort<T: Ord>(slice: &mut [T]) {
    let mut tmp = Vec::with_capacity(slice.len());
    for _ in 0..slice.len() {
        tmp.push(MaybeUninit::<T>::uninit());
    }

    // SAFETY: `MaybeUninit<T>` is `#[repr(transparent)]` which guarantees that
    //   it has the same layout as `T`. This in turn guarantees that `&mut [T]`
    //   and `&mut [MaybeUninit<T>]` have same layouts.
    let slice = unsafe {
        let len = slice.len();
        let ptr = slice.as_mut_ptr().cast::<MaybeUninit<T>>();
        core::slice::from_raw_parts_mut(ptr, len)
    };

    // SAFETY: all items in slice are initialized
    unsafe { merge_sort_core(slice, &mut tmp, 0) };
    // SAFETY:
    //  * `merge_sort_core` guarantees that all items in `slice` are initialized
    //     after it returns. Thus the original reference to slice is OK to be
    //     used now after we return.
    //  * `tmp` now contains only uninitialized data. Thus we don't need to drop
    //     any `T`s and `Vec` can safely `drop` itself.
    //
}

/// Sort initialized values into `output`.
///
/// As a result all items in `output` will be initialized
/// and all items in `tmp` will be uninitialized.
///
/// # SAFETY:
///
/// * outer call must start at `depth == 0`
/// * at even (including 0) `depth`, all items in `output` must be initialized
/// * at odd `depth`, all items in `tmp` must be initialized
unsafe fn merge_sort_core<T: Ord>(
    output: &mut [MaybeUninit<T>],
    tmp: &mut [MaybeUninit<T>],
    depth: usize,
) {
    if output.len() > 1 {
        let mid = output.len() / 2;
        let (l, r) = output.split_at_mut(mid);
        let (tmpl, tmpr) = tmp.split_at_mut(mid);

        // sort into temporary arrays

        // SAFETY: we alternate `tmp` and `output`.
        //  If at `depth==0` `output` is initialized,
        //  then at even depths `output` is initialized
        //  and at odd depths `tmp` is initialized.
        unsafe { merge_sort_core(tmpl, l, depth + 1) };
        unsafe { merge_sort_core(tmpr, r, depth + 1) };

        // merge into actual array we want to sort
        unsafe { merge(output, tmpl, tmpr) };
    } else if depth % 2 != 0 {
        // odd depth with single item
        // tmp is initialized, swap with output
        mem::swap(&mut output[0], &mut tmp[0])
    } else {
        // even depth with single item
        // output is already sorted and initialized
    }
}

/// Merge sorted slices l and r into output.
///
/// Note that following must hold: `l.len() + r.len() == output.len()`.
///
/// As a result all items in output will be initialized and sorted.
/// All items in l and r will be uninitialized.
///
/// # SAFETY
///
/// * all items in l and r must be initialized at start
unsafe fn merge<T: Ord>(
    output: &mut [MaybeUninit<T>],
    l: &mut [MaybeUninit<T>],
    r: &mut [MaybeUninit<T>],
) {
    debug_assert_eq!(l.len() + r.len(), output.len());
    let mut l_iter = l.iter_mut();
    let mut r_iter = r.iter_mut();

    let mut l_head = l_iter.next();
    let mut r_head = r_iter.next();
    // take items from left and right one at the time
    // put the smaller of lhead and rhead as the next item in slice
    for it in output.iter_mut() {
        match (&mut l_head, &mut r_head) {
            (None, None) => unreachable!(),
            (None, Some(r)) => {
                mem::swap(it, r);
                r_head = r_iter.next();
            }
            (Some(l), None) => {
                mem::swap(it, l);
                l_head = l_iter.next();
            }
            (Some(l), Some(r)) => {
                if unsafe { l.assume_init_ref() <= r.assume_init_ref() } {
                    mem::swap(it, l);
                    l_head = l_iter.next();
                } else {
                    mem::swap(it, r);
                    r_head = r_iter.next();
                }
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
    #[cfg_attr(miri, ignore = "no unsafe code, nothing for miri to check")]
    fn merge_sort_copy_test() {
        let mut arr = vec![1, 4, 2, 24, 65, 3, 3, 45];
        let mut sorted = arr.clone();
        sorted.sort();
        merge_sort_copy(arr.as_mut_slice());
        assert_eq!(arr, sorted);
    }

    #[test]
    fn merge_sort_test() {
        let mut arr = vec![1, 4, 2, 24, 65, 3, 3, 45];
        let mut sorted = arr.clone();
        sorted.sort();
        merge_sort(arr.as_mut_slice());
        assert_eq!(arr, sorted);
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
        const PROPTEST_CASES: u32 = 50;

        proptest!(
            #![proptest_config(ProptestConfig::with_cases(PROPTEST_CASES))]

            #[test]
            #[cfg_attr(miri, ignore = "no unsafe code, nothing for miri to check")]
            fn merge_sort_copy_test(
                mut vec in proptest::collection::vec(0..10000i32, 0..VEC_SIZE),
            ) {
               merge_sort_copy(vec.as_mut_slice());
               let mut sorted = vec.clone();
               sorted.sort();
               merge_sort_copy(vec.as_mut_slice());
               assert_eq!(vec, sorted);
            }

            #[test]
            fn merge_sort_test(
                mut vec in proptest::collection::vec(0..10000i32, 0..VEC_SIZE),
            ) {
               merge_sort_copy(vec.as_mut_slice());
               let mut sorted = vec.clone();
               sorted.sort();
               merge_sort(vec.as_mut_slice());
               assert_eq!(vec, sorted);
            }
        );
    }
}
