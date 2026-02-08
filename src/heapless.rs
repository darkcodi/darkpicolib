#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PushError;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HeaplessString<const N: usize> {
    length: u8,
    data: [u8; N],
}

impl<const N: usize> Default for HeaplessString<N> {
    fn default() -> Self {
        Self {
            length: 0,
            data: [0; N],
        }
    }
}

impl<const N: usize> core::fmt::Display for HeaplessString<N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = core::str::from_utf8(&self.data[..self.length as usize])
            .map_err(|_| core::fmt::Error)?;
        write!(f, "{}", s)
    }
}

impl<const N: usize> TryFrom<&str> for HeaplessString<N> {
    type Error = PushError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let bytes = value.as_bytes();
        if bytes.len() > N {
            return Err(PushError);
        }
        let mut data = [0u8; N];
        data[..bytes.len()].copy_from_slice(bytes);
        Ok(Self {
            length: bytes.len() as u8,
            data,
        })
    }
}

impl<const N: usize> HeaplessString<N> {
    pub const fn new() -> Self {
        Self {
            length: 0,
            data: [0; N],
        }
    }

    pub const fn repeat(c: char) -> Self {
        let mut s = Self::new();
        let ch_len = c.len_utf8();
        let mut i = 0;
        while i + ch_len <= N {
            let mut buf = [0u8; 4];
            c.encode_utf8(&mut buf);
            let mut j = 0;
            while j < ch_len {
                s.data[i + j] = buf[j];
                j += 1;
            }
            s.length += ch_len as u8;
            i += ch_len;
        }
        s
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.data[..self.length as usize]).unwrap_or("")
    }

    pub fn len(&self) -> usize {
        self.length as usize
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn capacity(&self) -> usize {
        N
    }

    pub fn clear(&mut self) {
        self.length = 0;
        self.data = [0; N];
    }

    pub fn push(&mut self, ch: char) -> Result<(), PushError> {
        let ch_len = ch.len_utf8();
        if (self.length as usize) + ch_len > N {
            return Err(PushError);
        }
        let mut buf = [0u8; 4];
        ch.encode_utf8(&mut buf);
        self.data[self.length as usize..self.length as usize + ch_len]
            .copy_from_slice(&buf[..ch_len]);
        self.length += ch_len as u8;
        Ok(())
    }

    pub fn push_str(&mut self, s: &str) -> Result<(), PushError> {
        let bytes = s.as_bytes();
        if (self.length as usize) + bytes.len() > N {
            return Err(PushError);
        }
        self.data[self.length as usize..self.length as usize + bytes.len()].copy_from_slice(bytes);
        self.length += bytes.len() as u8;
        Ok(())
    }
}

impl<const N: usize> core::fmt::Write for HeaplessString<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let remaining = N.saturating_sub(self.length as usize);
        let to_copy = bytes.len().min(remaining);

        self.data[self.length as usize..self.length as usize + to_copy]
            .copy_from_slice(&bytes[..to_copy]);
        self.length += to_copy as u8;

        Ok(())
    }

    fn write_char(&mut self, c: char) -> core::fmt::Result {
        let mut buf = [0u8; 4];
        let encoded = c.encode_utf8(&mut buf);
        self.write_str(encoded)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HeaplessVec<T, const N: usize> {
    length: u16,
    data: [T; N],
}

impl<T: Default, const N: usize> Default for HeaplessVec<T, N> {
    fn default() -> Self {
        Self {
            length: 0,
            data: core::array::from_fn(|_| T::default()),
        }
    }
}

impl<T, const N: usize> HeaplessVec<T, N> {
    pub fn len(&self) -> usize {
        self.length as usize
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn capacity(&self) -> usize {
        N
    }

    pub fn clear(&mut self) {
        self.length = 0;
    }

    pub fn push(&mut self, item: T) -> Result<(), PushError> {
        if (self.length as usize) >= N {
            return Err(PushError);
        }
        self.data[self.length as usize] = item;
        self.length += 1;
        Ok(())
    }

    pub fn pop(&mut self) -> Option<T>
    where
        T: Default,
    {
        if self.length == 0 {
            return None;
        }
        self.length -= 1;

        Some(core::mem::take(&mut self.data[self.length as usize]))
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        if index < self.length as usize {
            Some(&self.data[index])
        } else {
            None
        }
    }

    pub fn as_slice(&self) -> &[T] {
        &self.data[..self.length as usize]
    }

    pub fn extend_from_slice(&mut self, slice: &[T]) -> Result<(), PushError>
    where
        T: Clone,
    {
        if (self.length as usize) + slice.len() > N {
            return Err(PushError);
        }

        for (i, item) in slice.iter().enumerate() {
            self.data[self.length as usize + i] = item.clone();
        }

        self.length += slice.len() as u16;
        Ok(())
    }

    pub fn retain<F>(&mut self, mut f: F)
    where
        F: FnMut(&T) -> bool,
        T: Clone,
    {
        let mut write_idx = 0;
        let len = self.length as usize;

        for read_idx in 0..len {
            if f(&self.data[read_idx]) {
                if write_idx != read_idx {
                    self.data[write_idx] = self.data[read_idx].clone();
                }
                write_idx += 1;
            }
        }

        self.length = write_idx as u16;
    }

    pub fn remove(&mut self, index: usize)
    where
        T: Clone,
    {
        if index >= self.length as usize {
            panic!("remove index out of bounds");
        }

        for i in index..self.length as usize - 1 {
            self.data[i] = self.data[i + 1].clone();
        }

        self.length -= 1;
    }

    pub fn dequeue_front(&mut self) -> Option<T>
    where
        T: Clone,
    {
        if self.length == 0 {
            return None;
        }

        let item = self.data[0].clone();

        for i in 0..self.length as usize - 1 {
            self.data[i] = self.data[i + 1].clone();
        }

        self.length -= 1;
        Some(item)
    }

    pub fn front(&self) -> Option<&T> {
        if self.length == 0 {
            None
        } else {
            Some(&self.data[0])
        }
    }
}

impl<T: Default, const N: usize> HeaplessVec<T, N> {
    pub fn new() -> Self {
        Self {
            length: 0,
            data: core::array::from_fn(|_| T::default()),
        }
    }
}

impl<const N: usize> HeaplessVec<u8, N> {
    pub const fn const_new() -> Self {
        Self {
            length: 0,
            data: [0u8; N],
        }
    }

    pub const fn repeat(n: u8) -> Self {
        let mut vec = Self::const_new();
        let mut i = 0;
        while i < N {
            vec.data[i] = n;
            vec.length += 1;
            i += 1;
        }
        vec
    }
}

impl<T, const N: usize> core::ops::Deref for HeaplessVec<T, N> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.data[..self.length as usize]
    }
}

impl<T, const N: usize> core::ops::DerefMut for HeaplessVec<T, N> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.data[..self.length as usize]
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a HeaplessVec<T, N> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

impl<'a, T, const N: usize> IntoIterator for &'a mut HeaplessVec<T, N> {
    type Item = &'a mut T;
    type IntoIter = core::slice::IterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data[..self.length as usize].iter_mut()
    }
}

impl<T: Default, const N: usize> IntoIterator for HeaplessVec<T, N> {
    type Item = T;
    type IntoIter = HeaplessVecIntoIterator<T, N>;

    fn into_iter(self) -> Self::IntoIter {
        HeaplessVecIntoIterator {
            vec: self,
            index: 0,
        }
    }
}

pub struct HeaplessVecIntoIterator<T: Default, const N: usize> {
    vec: HeaplessVec<T, N>,
    index: u16,
}

impl<T: Default, const N: usize> Iterator for HeaplessVecIntoIterator<T, N> {
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.vec.length {
            return None;
        }

        let item = core::mem::take(&mut self.vec.data[self.index as usize]);
        self.index += 1;

        if self.index == self.vec.length {
            self.vec.length = 0;
        }

        Some(item)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct HeaplessQueue<T, const N: usize> {
    length: u16,
    head: u16,
    tail: u16,
    data: [T; N],
}

impl<T: Default, const N: usize> Default for HeaplessQueue<T, N> {
    fn default() -> Self {
        Self {
            length: 0,
            head: 0,
            tail: 0,
            data: core::array::from_fn(|_| T::default()),
        }
    }
}

impl<T: Default, const N: usize> HeaplessQueue<T, N> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T, const N: usize> HeaplessQueue<T, N> {
    pub fn len(&self) -> usize {
        self.length as usize
    }

    pub fn is_empty(&self) -> bool {
        self.length == 0
    }

    pub fn is_full(&self) -> bool {
        self.length as usize == N
    }

    pub fn capacity(&self) -> usize {
        N
    }

    pub fn clear(&mut self) {
        self.length = 0;
        self.head = 0;
        self.tail = 0;
    }

    pub fn enqueue(&mut self, item: T) -> Result<(), PushError> {
        if self.is_full() {
            return Err(PushError);
        }

        self.data[self.tail as usize] = item;
        self.tail = if (self.tail as usize) + 1 == N {
            0
        } else {
            self.tail + 1
        };
        self.length += 1;

        Ok(())
    }

    pub fn dequeue(&mut self) -> Option<T>
    where
        T: Default,
    {
        if self.is_empty() {
            return None;
        }

        let item = core::mem::take(&mut self.data[self.head as usize]);
        self.head = if (self.head as usize) + 1 == N {
            0
        } else {
            self.head + 1
        };
        self.length -= 1;

        Some(item)
    }

    pub fn front(&self) -> Option<&T> {
        if self.is_empty() {
            None
        } else {
            Some(&self.data[self.head as usize])
        }
    }

    pub fn back(&self) -> Option<&T> {
        if self.is_empty() {
            return None;
        }

        let last_idx = if self.tail == 0 {
            N - 1
        } else {
            self.tail as usize - 1
        };

        Some(&self.data[last_idx])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Write;
    use core::mem::size_of;

    #[test]
    fn test_optimized_string_size() {
        assert_eq!(size_of::<HeaplessString<0>>(), 1);
        assert_eq!(size_of::<HeaplessString<1>>(), 2);
        assert_eq!(size_of::<HeaplessString<2>>(), 3);
        assert_eq!(size_of::<HeaplessString<3>>(), 4);
        assert_eq!(size_of::<HeaplessString<4>>(), 5);
        assert_eq!(size_of::<HeaplessString<5>>(), 6);
        assert_eq!(size_of::<HeaplessString<6>>(), 7);
        assert_eq!(size_of::<HeaplessString<7>>(), 8);
        assert_eq!(size_of::<HeaplessString<8>>(), 9);
        assert_eq!(size_of::<HeaplessString<9>>(), 10);
        assert_eq!(size_of::<HeaplessString<10>>(), 11);
        assert_eq!(size_of::<HeaplessString<11>>(), 12);
        assert_eq!(size_of::<HeaplessString<12>>(), 13);
        assert_eq!(size_of::<HeaplessString<13>>(), 14);
        assert_eq!(size_of::<HeaplessString<14>>(), 15);
        assert_eq!(size_of::<HeaplessString<15>>(), 16);
        assert_eq!(size_of::<HeaplessString<16>>(), 17);
        assert_eq!(size_of::<HeaplessString<17>>(), 18);
        assert_eq!(size_of::<HeaplessString<18>>(), 19);
        assert_eq!(size_of::<HeaplessString<19>>(), 20);
        assert_eq!(size_of::<HeaplessString<20>>(), 21);
        assert_eq!(size_of::<HeaplessString<21>>(), 22);
        assert_eq!(size_of::<HeaplessString<22>>(), 23);
        assert_eq!(size_of::<HeaplessString<23>>(), 24);
        assert_eq!(size_of::<HeaplessString<24>>(), 25);
        assert_eq!(size_of::<HeaplessString<25>>(), 26);
        assert_eq!(size_of::<HeaplessString<26>>(), 27);
        assert_eq!(size_of::<HeaplessString<27>>(), 28);
        assert_eq!(size_of::<HeaplessString<28>>(), 29);
        assert_eq!(size_of::<HeaplessString<29>>(), 30);
        assert_eq!(size_of::<HeaplessString<30>>(), 31);
        assert_eq!(size_of::<HeaplessString<31>>(), 32);
        assert_eq!(size_of::<HeaplessString<32>>(), 33);
    }

    #[test]
    fn test_extend_from_slice() {
        let mut vec: HeaplessVec<u8, 5> = HeaplessVec::new();
        let _ = vec.push(1);
        let _ = vec.push(2);

        assert!(vec.extend_from_slice(&[]).is_ok());
        assert_eq!(vec.len(), 2);

        assert!(vec.extend_from_slice(&[3, 4]).is_ok());
        assert_eq!(vec.len(), 4);
        assert_eq!(vec.as_slice(), &[1, 2, 3, 4]);

        assert!(vec.extend_from_slice(&[5, 6]).is_err());
        assert_eq!(vec.len(), 4);
    }

    #[test]
    fn test_remove() {
        let mut vec: HeaplessVec<u8, 5> = HeaplessVec::new();
        let _ = vec.push(1);
        let _ = vec.push(2);
        let _ = vec.push(3);
        let _ = vec.push(4);

        vec.remove(1);
        assert_eq!(vec.as_slice(), &[1, 3, 4]);

        vec.remove(0);
        assert_eq!(vec.as_slice(), &[3, 4]);

        vec.remove(1);
        assert_eq!(vec.as_slice(), &[3]);

        vec.remove(0);
        assert!(vec.is_empty());
    }

    #[test]
    #[should_panic(expected = "remove index out of bounds")]
    fn test_remove_out_of_bounds() {
        let mut vec: HeaplessVec<u8, 5> = HeaplessVec::new();
        vec.push(1).unwrap();
        vec.remove(5);
    }

    #[test]
    fn test_retain() {
        let mut vec: HeaplessVec<u8, 10> = HeaplessVec::new();
        for i in 1..=5 {
            let _ = vec.push(i);
        }

        vec.retain(|x| x % 2 == 0);
        assert_eq!(vec.as_slice(), &[2, 4]);

        vec.retain(|_| true);
        assert_eq!(vec.as_slice(), &[2, 4]);

        vec.retain(|_| false);
        assert!(vec.is_empty());
    }

    #[test]
    fn test_vec_iteration_via_deref() {
        let mut vec: HeaplessVec<u8, 5> = HeaplessVec::new();
        let _ = vec.push(1);
        let _ = vec.push(2);
        let _ = vec.push(3);

        let mut iter = vec.iter();
        assert_eq!(iter.next(), Some(&1));
        assert_eq!(iter.next(), Some(&2));
        assert_eq!(iter.next(), Some(&3));
        assert_eq!(iter.next(), None);

        let mut sum = 0;
        for item in &vec {
            sum += item;
        }
        assert_eq!(sum, 6);
    }

    #[test]
    fn test_fmt_write_basic() {
        let mut s = HeaplessString::<10>::new();
        write!(s, "hello").unwrap();
        assert_eq!(s.as_str(), "hello");
        assert_eq!(s.len(), 5);
    }

    #[test]
    fn test_fmt_write_formatting() {
        let mut s = HeaplessString::<20>::new();
        write!(s, "Hello, world!").unwrap();
        assert_eq!(s.as_str(), "Hello, world!");
    }

    #[test]
    fn test_fmt_write_truncates_on_overflow() {
        let mut s = HeaplessString::<5>::new();
        write!(s, "hello world").unwrap();

        assert_eq!(s.as_str(), "hello");
        assert_eq!(s.len(), 5);
    }

    #[test]
    fn test_queue_fifo_and_capacity() {
        let mut queue: HeaplessQueue<u8, 3> = HeaplessQueue::new();
        assert!(queue.is_empty());
        assert_eq!(queue.capacity(), 3);
        assert_eq!(queue.front(), None);
        assert_eq!(queue.back(), None);

        assert!(queue.enqueue(1).is_ok());
        assert!(queue.enqueue(2).is_ok());
        assert!(queue.enqueue(3).is_ok());
        assert!(queue.is_full());
        assert_eq!(queue.front(), Some(&1));
        assert_eq!(queue.back(), Some(&3));

        assert_eq!(queue.enqueue(4), Err(PushError));

        assert_eq!(queue.dequeue(), Some(1));
        assert_eq!(queue.dequeue(), Some(2));
        assert_eq!(queue.dequeue(), Some(3));
        assert_eq!(queue.dequeue(), None);
    }

    #[test]
    fn test_queue_wraparound() {
        let mut queue: HeaplessQueue<u8, 3> = HeaplessQueue::new();

        assert!(queue.enqueue(1).is_ok());
        assert!(queue.enqueue(2).is_ok());
        assert!(queue.enqueue(3).is_ok());
        assert_eq!(queue.dequeue(), Some(1));
        assert_eq!(queue.dequeue(), Some(2));

        assert!(queue.enqueue(4).is_ok());
        assert!(queue.enqueue(5).is_ok());
        assert!(queue.is_full());
        assert_eq!(queue.front(), Some(&3));
        assert_eq!(queue.back(), Some(&5));

        assert_eq!(queue.dequeue(), Some(3));
        assert_eq!(queue.dequeue(), Some(4));
        assert_eq!(queue.dequeue(), Some(5));
        assert!(queue.is_empty());
    }

    #[test]
    fn test_queue_zero_capacity() {
        let mut queue: HeaplessQueue<u8, 0> = HeaplessQueue::new();
        assert!(queue.is_empty());
        assert!(queue.is_full());
        assert_eq!(queue.capacity(), 0);
        assert_eq!(queue.enqueue(1), Err(PushError));
        assert_eq!(queue.dequeue(), None);
    }
}
