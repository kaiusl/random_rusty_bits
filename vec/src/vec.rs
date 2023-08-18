extern crate alloc as crate_alloc;

use core::alloc::Layout;
use core::marker::PhantomData;
use core::ptr::NonNull;
use core::{fmt, mem, ptr, slice};

use crate_alloc::alloc;

struct Vec2<T> {
    // INVARIANTS:
    //  * `len <= cap <= isize::MAX`
    //  * first `len` elements in `buf` are initialized
    //  * `buf` is valid pointer to contiguous memory to store `cap` `T`s
    //    (`buf` can only be `NonNull::dangling` if `cap == len == 0`)
    //  * we never allocate more than `isize::MAX` bytes, that is
    //    `cap * mem::size_of::<T>() <= isize::MAX`
    buf: NonNull<T>,
    len: usize,
    cap: usize,
    marker: PhantomData<T>,
}

impl<T> fmt::Debug for Vec2<T>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vec2")
            .field("len", &self.len)
            .field("cap", &self.cap)
            .field("buf", &self.as_slice())
            .finish()
    }
}

impl<T> Drop for Vec2<T> {
    fn drop(&mut self) {
        if self.cap == 0 {
            return;
        }

        /// Drop guard in case T::drop panics.
        ///
        /// In the case on unwinding we try to drop the remaining items.
        /// If that succeeds we deallocate our buffer and the caller could catch the unwinding,
        /// if not we abort due to double panic.
        struct Guard<'a, U>(&'a mut Vec2<U>);

        impl<'a, U> Drop for Guard<'a, U> {
            fn drop(&mut self) {
                while self.0.pop().is_some() {}

                assert_eq!(self.0.len, 0);

                let layout = self.0.current_layout();
                self.0.cap = 0;
                let buf = mem::replace(&mut self.0.buf, NonNull::dangling())
                    .as_ptr()
                    .cast::<u8>();

                unsafe { alloc::dealloc(buf, layout) };
            }
        }

        let g = Guard(self);
        while g.0.pop().is_some() {}
    }
}

impl<T> Vec2<T> {
    // Notes:
    //  * On any allocation error we panic for now
    //    TODO: add try_grow methods
    const INITIAL_CAP: usize = 2;

    pub fn new() -> Self {
        assert!(mem::size_of::<T>() != 0, "we don't (yet) support ZST");
        Self {
            // SAFETY: self.buf is never touched before actually initializing it
            buf: NonNull::dangling(),
            len: 0,
            cap: 0,
            marker: PhantomData,
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        let mut s = Self::new();
        s.grow_to(cap);
        s
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn as_slice(&self) -> &[T] {
        // SAFETY:
        //  * if `len == cap == 0` then `self.buf == NonNull::dangling`,
        //    this is valid pointer for zero-len slice (see docs of `slice::from_raw_parts`)
        //  * otherwise `self.buf` is a valid pointer to `self.len` `T`s
        //    gotten from `alloc::alloc` with `Layout::array<T>(cap)` which is non-null and properly aligned.
        //    First `self.len` `T`s in that memory are properly initialized.
        unsafe { slice::from_raw_parts(self.buf.as_ptr().cast_const(), self.len) }
    }

    pub fn push(&mut self, val: T) {
        if self.len == self.cap {
            self.grow()
        }

        assert!(self.len < self.cap);
        // SAFETY:
        //  * self.len < self.cap, is in bounds
        //  * `ptr` points to the first uninitialized `T` and thus `self.len + 1`
        //    first items will be initialized after this write
        unsafe {
            self.write_at(self.len, val);
            self.set_len(self.len + 1);
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        // Want to read at last index, so decrement before reading
        self.len -= 1;
        // SAFETY:
        //  * self.len = orig_len - 1 is the index of last item, is in bounds,
        //  * no-one has references to this item
        //  * this item will never be read again, only written over
        let val = unsafe { self.read_at(self.len) };
        Some(val)
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        if !self.is_in_bounds(index) {
            return None;
        }

        // SAFETY: index is in bounds (checked above)
        let ptr = unsafe { self.get_raw_unchecked(index) };
        // SAFETY:
        //  * lifetime of returned reference is bound to the borrow of `self`, is must remain alive for '0
        //  * `ptr` is non-null as self.buf is non-null
        //  * `ptr` is properly aligned because self.buf is and ptr::add keeps it aligned
        //  * `ptr` points to a initialized T since `index < self.len` and first
        //    `self.len` items in `self.buf` are initialized (see INVARIANTS in struct definition)
        unsafe { Some(&*ptr) }
    }

    pub fn remove(&mut self, index: usize) -> Option<T> {
        if !self.is_in_bounds(index) {
            return None;
        }

        // SAFETY:
        //  * index is in bounds (checked above) and no-one has references to it
        //  * this item will never be read again, only written over
        let val = unsafe { self.read_at(index) };

        // shift tail down by 1 position
        // [head] [empty_slot] [tail]     [after]
        //        ^-index      ^-index+1  ^-self.len
        self.len -= 1;
        // Number of items in tail: if we removed the last item then index = orig_len - 1 = self.len.
        // In that case tail_count must equal 0, thus tail_count = self.len - index = orig_len - 1 - index
        let tail_count = self.len - index;
        if tail_count > 0 {
            // SAFETY:
            //  * [index + 1, index + 1 + tail_count = self.len + 1 = orig_len) items are initialized and valid to be read (tail items)
            //  * by taking `&mut self` we invalidate any previously returned references, whole buffer is valid to be written to.
            //  * since amount == -1 and index is in bounds, dst must be in bounds
            unsafe { self.shift_items(index + 1, tail_count, -1) }
        }

        // SAFETY:
        //  * we have shifted down the tail so at this point again self.len first
        //    items in self.buf are initialized and all our invariants hold

        Some(val)
    }

    pub fn insert(&mut self, index: usize, val: T) -> Result<(), T> {
        if index > self.len {
            // index == self.len is ok here, it's equivalent to self.push
            return Err(val);
        }

        if index == self.len {
            self.push(val);
            return Ok(());
        }

        if self.len == self.cap {
            self.grow()
        }

        assert!(self.len < self.cap);

        let tail_count = self.len - index;
        // SAFETY:
        //  * [index, index + tail_count = self.len) items are initialized,
        //    previous references and invalidated and thus valid to be read
        //  * we checked that there is room for one more item,
        //    thus items at [index + 1, index + tail_count + 1 = self.len + 1 <= self.cap) are valid to be written to
        unsafe { self.shift_items(index, tail_count, 1) }

        // SAFETY:
        //  * `index < self.cap`, is in bounds
        //  * previous item at `index` was shifted away, `index` is an empty slot
        unsafe { self.write_at(index, val) }

        // SAFETY:
        //  * as we moved [index, self.len) items up by one and filled the gap at index,
        //    `self.len + 1` first items are now initialized
        unsafe { self.set_len(self.len + 1) };

        Ok(())
    }

    /// # SAFETY
    ///
    ///  * first `new_len` elements in `self.buf` must be properly initialized
    unsafe fn set_len(&mut self, new_len: usize) {
        self.len = new_len
    }

    /// # SAFETY
    ///
    /// New buffer must uphold the invariants of our type (see type definition).
    ///
    /// This means that:
    /// * `new_buf` is valid pointer to contiguous memory to store `new_cap` `T`s
    ///    (it can only be `NonNull::dangling` if `new_cap == self.len == 0`)
    /// * first `self.len` elements in `new_buf` must be properly initialized
    /// * `self.len <= new_cap <= isize::MAX`
    unsafe fn set_buf(&mut self, new_buf: NonNull<T>, new_cap: usize) {
        self.buf = new_buf;
        self.cap = new_cap;
    }

    #[inline(always)]
    fn is_in_bounds(&self, index: usize) -> bool {
        index < self.len
    }

    /// Returns a pointer to item at `index` in `self.buf`.
    ///
    /// The returned pointer is non-null and properly aligned.
    /// The pointed item may be uninitialized.
    ///
    /// # SAFETY
    ///
    /// * `index` must be in bounds of buffer (`index < self.cap`)
    unsafe fn get_raw_unchecked(&self, index: usize) -> *mut T {
        // SAFETY:
        //  * `self.buf` is valid pointer for `self.cap >= self.len > index`
        //    `T`s so the resulting pointer is in bounds
        //  * computed offset `index * mem::size_of::<T>() < isize::MAX`
        //    because our allocation size `self.cap * mem::size_of::<T>()`
        //    is checked to be `< isize::MAX` in allocation code (see `self.grow_to`)
        unsafe { self.buf.as_ptr().add(index) }
    }

    /// Write `val` at `index`.
    ///
    /// # SAFETY
    ///
    /// * `index < self.cap`
    /// * item at `index` must be valid to be written to
    unsafe fn write_at(&mut self, index: usize, val: T) {
        let ptr = unsafe { self.get_raw_unchecked(index) };
        // SAFETY:
        //  * get_raw_unchecked return non-null and properly aligned pointers into self.buf
        //  * any references given out before are invalidated by taking
        //    `&mut self` (all returned references are bound to a borrow of `self`)
        unsafe { ptr.write(val) };
    }

    /// Read the item at `index`.
    ///
    /// # SAFETY
    ///
    /// * item at `index` must be valid to be read
    /// * item at `index` must never be read from again
    unsafe fn read_at(&mut self, index: usize) -> T {
        // SAFETY: index is in bounds
        let ptr = unsafe { self.get_raw_unchecked(index) };
        // SAFETY:
        //  * this item will never be read again, only written over
        //  * `ptr` is valid to be read from
        //    - get_raw_unchecked return non-null and properly aligned pointers
        //    - any references given out before are invalidated by taking
        //      `&mut self` (all returned references are bound to a borrow of `self`)
        //  * `ptr` points to a properly initialized `T` since first `self.len`
        //    items in `self.buf` are initialized (see INVARIANTS in struct definition)
        unsafe { ptr.read() }
    }

    /// # SAFETY
    ///
    /// * src = [start, start + count) must be initialized items valid to be read
    /// * dst = [start + amount, start + amount + count) must be valid to be written to
    unsafe fn shift_items(&mut self, start: usize, count: usize, amount: isize) {
        unsafe {
            // SAFETY: start < self.cap
            let src = self.get_raw_unchecked(start);
            // SAFETY: 0 <= start + amount < self.cap
            let dst = src.offset(amount);
            // SAFETY:
            //  * src and dst may overlap, use ptr::copy
            //  * `src` and `dst` are properly aligned and non-null
            //  * `src` is valid for count reads because self.buf must have at least start + count initialized items
            //  * `dst` is valid for count writes because self.buf has memory for at least start + amount + count items
            ptr::copy(src, dst, count)
        }
    }

    #[inline]
    fn current_layout(&self) -> Layout {
        // This cannot return Err variant as we have already checked it
        Layout::array::<T>(self.cap).unwrap()
    }

    fn grow_to(&mut self, new_cap: usize) {
        if new_cap <= self.cap {
            return;
        }

        let (buf, layout) = if self.cap == 0 {
            let layout = Layout::array::<T>(new_cap).unwrap();
            debug_assert_ne!(layout.size(), 0);
            // SAFETY: `new_cap * mem::size_of<T>() > 0` because `new_cap > 0`
            //  (new_cap > cap == 0 by combining two if statements) and we
            //  don't support ZST
            let buf = unsafe { alloc::alloc(layout) };
            (buf, layout)
        } else {
            let new_layout = Layout::array::<T>(new_cap).unwrap();
            // SAFETY:
            //  * we allocate only with Global allocator (we don't support custom allocators)
            //  * `self.current_layout()` returns the layout of current `self.buf`
            //  * `new_size = new_layout.size() > 0` because (`new_cap > cap != 0`) and we don't support ZST
            //  * `new_size = new_layout.size() < isize::MAX` because `Layout::array` would panic if this is not the case.
            let buf = unsafe {
                alloc::realloc(
                    self.buf.as_ptr().cast::<u8>(),
                    self.current_layout(),
                    new_layout.size(),
                )
            };
            (buf, new_layout)
        };

        if buf.is_null() {
            alloc::handle_alloc_error(layout)
        } else {
            // SAFETY:
            //  * we just checked that buf is not null.
            let new_buf = unsafe { NonNull::new_unchecked(buf.cast::<T>()) };
            // SAFETY:
            //  * `new_buf` is allocated with Layout::array::<T>(new_cap) which
            //    is properly aligned (by alloc::alloc) and non-null pointer to
            //    contiguous memory to store `new_cap` `T`s
            //  * If there were items in previous buffer, they have all been
            //    moved into the new buffer.
            //  * `new_cap <= isize::MAX` because otherwise `Layout::array` would panic
            unsafe { self.set_buf(new_buf, new_cap) }
        }
    }

    fn grow(&mut self) {
        let new_cap = if self.cap == 0 {
            Self::INITIAL_CAP
        } else {
            // Cannot overflow because Layout::array constraints the total
            // number of bytes allocated to be less than isize::MAX.
            // Thus at most self.cap == isize::MAX and isize::MAX * 2 == usize::MAX - 1
            self.cap * 2
        };
        self.grow_to(new_cap);
    }
}

#[cfg(test)]
mod tests {
    use core::panic::AssertUnwindSafe;
    use core::sync::atomic::AtomicUsize;
    use std::panic::catch_unwind;

    use super::*;

    fn covariant<'a, T>(a: Vec2<&'static T>) -> Vec2<&'a T> {
        a
    }

    #[test]
    fn it_works() {
        let mut v = Vec2::new();
        assert!(v.is_empty());
        v.push(2);
        assert_eq!(v.len(), 1);
        v.push(3);
        assert_eq!(v.len(), 2);
        v.push(4);
        assert_eq!(v.len(), 3);
        assert_eq!(v.as_slice(), &[2, 3, 4]);

        assert_eq!(v.pop(), Some(4));
        assert_eq!(v.len(), 2);
        assert_eq!(v.pop(), Some(3));
        assert_eq!(v.len(), 1);
        v.insert(1, 5).unwrap();
        assert_eq!(v.len(), 2);
        v.insert(1, 6).unwrap();
        assert_eq!(v.len(), 3);
        assert_eq!(v.as_slice(), &[2, 6, 5]);

        assert_eq!(v.remove(1), Some(6));
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn it_works2() {
        let mut v = Vec2::new();
        v.push(String::from("2"));
        v.push(String::from("3"));
        v.push(String::from("4"));

        v.pop();
        v.pop();
    }

    #[test]
    fn get() {
        let mut v = Vec2::new();
        v.push(2);
        v.push(3);
        v.push(4);

        assert_eq!(v.get(0), Some(&2));
        assert_eq!(v.get(1), Some(&3));
        assert_eq!(v.get(2), Some(&4));
        assert_eq!(v.get(3), None);
    }

    #[test]
    fn remove() {
        let mut v = Vec2::new();
        assert_eq!(v.remove(0), None);

        v.push(2);
        v.push(3);
        v.push(4);
        v.push(5);
        v.push(6);
        v.push(7);

        assert_eq!(v.remove(0), Some(2)); // first
        assert_eq!(v.remove(v.len()), None); // past end
        assert_eq!(v.remove(v.len() - 1), Some(7)); // last
        assert_eq!(v.remove(1), Some(4)); // middle
    }

    #[test]
    fn insert() {
        let mut v = Vec2::new();
        assert_eq!(v.insert(1, 1), Err(1));
        v.insert(0, 1).unwrap(); // start
        v.insert(1, 2).unwrap(); // end
        v.insert(1, 3).unwrap(); // middle
        assert_eq!(v.as_slice(), &[1, 3, 2])
    }

    #[test]
    fn pop() {
        let mut v = Vec2::new();
        assert_eq!(v.pop(), None);
        v.push(2);
        v.push(3);
        assert_eq!(v.pop(), Some(3));
        assert_eq!(v.pop(), Some(2));
        assert_eq!(v.pop(), None);
    }

    #[test]
    fn panic_in_drop() {
        static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
        struct D(bool, String);

        impl Drop for D {
            fn drop(&mut self) {
                DROP_COUNT.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
                if self.0 {
                    panic!("panic from drop")
                }
            }
        }

        let mut v = Vec2::new();
        v.push(D(false, String::from("a")));
        v.push(D(true, String::from("b")));
        v.push(D(false, String::from("c")));

        catch_unwind(AssertUnwindSafe(|| drop(v))).ok();
        assert_eq!(DROP_COUNT.load(core::sync::atomic::Ordering::SeqCst), 3)
    }

    #[test]
    #[ignore = "should abort, needs to be manually checked"]
    fn panic_in_drop_abort() {
        static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);
        struct D(bool, String);

        impl Drop for D {
            fn drop(&mut self) {
                DROP_COUNT.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
                if self.0 {
                    panic!("panic from drop")
                }
            }
        }

        let mut v = Vec2::new();
        v.push(D(false, String::from("a")));
        v.push(D(true, String::from("b")));
        v.push(D(false, String::from("c")));
        v.push(D(true, String::from("d")));

        catch_unwind(AssertUnwindSafe(|| drop(v))).ok();
        assert_eq!(DROP_COUNT.load(core::sync::atomic::Ordering::SeqCst), 3)
    }
}
