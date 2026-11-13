//! Resource tables indexed by id (ids are small and handed out in order): the lookups a draw
//! needs are array accesses instead of hash map probes. The API mirrors the HashMap calls the
//! renderer used.

use std::marker::PhantomData;

/// An id that indexes a `Slab`.
pub(crate) trait SlabKey: Copy {
    fn index(&self) -> usize;
}

pub(crate) struct Slab<K, T> {
    items: Vec<Option<T>>,
    key: PhantomData<K>,
}

impl<K: SlabKey, T> Slab<K, T> {
    pub fn new() -> Slab<K, T> {
        Slab { items: Vec::new(), key: PhantomData }
    }

    #[inline]
    pub fn get(&self, k: &K) -> Option<&T> {
        self.items.get(k.index()).and_then(|s| s.as_ref())
    }

    #[inline]
    #[allow(dead_code)]
    pub fn get_mut(&mut self, k: &K) -> Option<&mut T> {
        self.items.get_mut(k.index()).and_then(|s| s.as_mut())
    }

    #[inline]
    pub fn contains_key(&self, k: &K) -> bool {
        self.get(k).is_some()
    }

    pub fn insert(&mut self, k: K, v: T) -> Option<T> {
        let i = k.index();
        if i >= self.items.len() {
            self.items.resize_with(i + 1, || None);
        }
        self.items[i].replace(v)
    }

    pub fn remove(&mut self, k: &K) -> Option<T> {
        self.items.get_mut(k.index()).and_then(|s| s.take())
    }
}

impl<K: SlabKey, T> std::ops::Index<&K> for Slab<K, T> {
    type Output = T;
    #[inline]
    fn index(&self, k: &K) -> &T {
        self.get(k).expect("no such resource")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    struct Id(u32);
    impl SlabKey for Id {
        fn index(&self) -> usize {
            self.0 as usize
        }
    }

    #[test]
    fn insert_get_remove() {
        let mut s: Slab<Id, &str> = Slab::new();
        assert!(s.get(&Id(3)).is_none());
        assert!(s.insert(Id(3), "a").is_none());
        assert_eq!(s.insert(Id(3), "b"), Some("a"));
        assert!(s.contains_key(&Id(3)) && !s.contains_key(&Id(2)) && !s.contains_key(&Id(9)));
        assert_eq!(s[&Id(3)], "b");
        *s.get_mut(&Id(3)).unwrap() = "c";
        assert_eq!(s.remove(&Id(3)), Some("c"));
        assert!(s.remove(&Id(3)).is_none() && s.remove(&Id(100)).is_none());
    }
}
