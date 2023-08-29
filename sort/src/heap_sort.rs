// Indices in a heap
//           0
//     1            2
//  3     4      5     6
// 7 8   9 10  11 12 13 14
//
// parent_index = (child_index - 1) / 2
// left_child = parent_index * 2 + 1
// right_child = left_child + 1 = parent_index * 2 + 2

pub fn heap_sort<T: Ord>(slice: &mut [T]) {
    build_max_heap(slice);

    for i in (1..slice.len()).rev() {
        // slice[..=i] is a max-heap, slice[0] is the largest item
        // slice[i+1..] is sorted

        // Largest item in heap is first, swap with the last unsorted item
        slice.swap(i, 0);
        // slice[i..] is now sorted

        // Swap ruined our heap by moving smaller item to the front,
        // shift it down to restore heap
        // both child trees are still proper heaps
        shift_down(&mut slice[..i], 0);
    }
}

/// Build a max-heap from any slice in-place.
fn build_max_heap<T: Ord>(slice: &mut [T]) {
    if slice.len() < 2 {
        // empty or 1-element slice, is already a heap
        return;
    }
    // Go through all parent nodes and shift them down starting from the bottom.
    // This will build up the max-heap bottom up.
    // We don't need to go through the last level since they don't have children and
    // a single item is proper heap already.
    //
    // A parent node is at (child_index - 1)/2.
    // Thus the last_parent is at index (last_index - 1)/2 = (slice.len() - 1 - 1)/2
    let last_parent = (slice.len() - 2) / 2;
    for i in (0..=last_parent).rev() {
        shift_down(slice, i);
    }
}

/// Shift the item at `parent_index` (which may violate max-heap property) down
/// the tree to restore max-heap.
///
/// Assumes that both child trees of `parent` are proper max-heaps.
fn shift_down<T: Ord>(slice: &mut [T], mut parent_index: usize) {
    // * Find the largest value of parent, left child, right child.
    // * If parent was largest, whole tree starting from parent is a max-heap, we are done.
    // * If not, swap parent with the largest children.
    // * Since this swap may have ruined the max-heap property of the child,
    //   repeat the process using child as the parent.

    loop {
        let parent = &slice[parent_index];

        let left_index = 2 * parent_index + 1;
        let (largest, largest_index) = match slice.get(left_index) {
            Some(left) if left > parent => (left, left_index),
            Some(_) => (parent, parent_index),
            None => return, // parent has no children
        };

        let right_index = left_index + 1;
        let largest_index = match slice.get(right_index) {
            Some(right) if right > largest => right_index,
            _ => largest_index,
        };

        if largest_index != parent_index {
            slice.swap(parent_index, largest_index);
            parent_index = largest_index;
        } else {
            // parent was largest, we are done
            break;
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
        heap_sort(&mut arr);
        assert_sorted(&arr);
    }

    #[test]
    #[cfg_attr(miri, ignore = "no unsafe code, nothing for miri to check")]
    fn test2() {
        let mut arr = vec![0, 0, 1];
        heap_sort(&mut arr);
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
               heap_sort(vec.as_mut_slice());
               assert_sorted(&vec);
            }

        );
    }
}
