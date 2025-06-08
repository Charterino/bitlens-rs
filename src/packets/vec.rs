#[derive(Clone)]
pub enum Vec<'a, T> {
    Bumpalod(bumpalo::collections::Vec<'a, T>),
    Heaped(std::vec::Vec<T>),
}

impl<'a, T> Vec<'a, T> {
    pub fn len(&self) -> usize {
        match self {
            Vec::Bumpalod(items) => items.len(),
            Vec::Heaped(items) => items.len(),
        }
    }

    pub fn get(&'a self, i: usize) -> Option<&'a T> {
        match self {
            Vec::Bumpalod(items) => items.get(i),
            Vec::Heaped(items) => items.get(i),
        }
    }

    pub fn as_slice(&'a self) -> &'a [T] {
        match self {
            Vec::Bumpalod(items) => items.as_slice(),
            Vec::Heaped(items) => items.as_slice(),
        }
    }

    pub fn reserve(&mut self, a: usize) {
        match self {
            Vec::Bumpalod(items) => items.reserve(a),
            Vec::Heaped(items) => items.reserve(a),
        }
    }

    pub fn push(&mut self, item: T) {
        match self {
            Vec::Bumpalod(items) => items.push(item),
            Vec::Heaped(items) => items.push(item),
        }
    }
}
