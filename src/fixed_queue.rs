use std::ops::Index;

#[derive(Debug)]
pub struct FixedQueue<T, const N: usize> {
    buf: Vec<T>,
    head: usize,
}

impl<T, const N: usize> Index<usize> for FixedQueue<T, N> {
    type Output = T;
    fn index(&self, index: usize) -> &Self::Output {
        &self.buf[(self.head + index) % self.buf.len()]
    }
}

impl<T, const N: usize> Default for FixedQueue<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> FixedQueue<T, N> {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(N),
            head: 0,
        }
    }

    pub fn push(&mut self, value: T) {
        if self.buf.len() < N {
            self.buf.push(value);
        } else {
            self.buf[self.head] = value;
            self.head = (self.head + 1) % N;
        }
    }
    pub fn len(&self) -> usize {
        self.buf.len()
    }
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}
