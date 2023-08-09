#![allow(dead_code)]
#![deny(rust_2018_idioms)]
#![deny(unsafe_op_in_unsafe_fn)]

pub fn bubble_sort(slice: &mut [i32]) {
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

    #[test]
    fn it_works() {
        let mut arr = vec![1, 4, 2, 24, 65, 3, 3, 45];
        bubble_sort(arr.as_mut_slice());
        println!("{:?}", arr);

        //let result = add(2, 2);
        //assert_eq!(result, 4);
    }
}
