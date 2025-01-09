/// `VecMap` is a classic map, that uses usize as keys, and it assumes that all the keys
/// are located extremely close to each other. It looks like a vector with "holes".
///
/// As opposed to `Slab` it doesn't create keys.
pub(crate) struct VecMap<V> {
    inner: Vec<Option<V>>,
}

impl<V> VecMap<V> {
    /// Creates a new instance of `VecMap`.
    pub(crate) const fn new() -> Self {
        Self { inner: Vec::new() }
    }

    /// Returns a mutable reference to the value associated with the key.
    pub(crate) fn get_mut(&mut self, key: usize) -> Option<&mut V> {
        self.inner.get_mut(key).and_then(|v| v.as_mut())
    }

    /// Inserts a new key-value pair into the map.
    ///
    /// Returns `None` or previous value.
    pub(crate) fn insert(&mut self, key: usize, value: V) -> Option<V> {
        debug_assert!(
            key < self.inner.len() + 100,
            "The provided key is too far from the previous one."
        );

        if self.inner.len() <= key {
            let new_len = (key + 1).max(self.inner.len() * 12 / 10);
            for _ in self.inner.len()..new_len {
                self.inner.push(None);
            }
        }

        if let Some(v) = self.inner.get_mut(key) {
            v.replace(value)
        } else {
            self.inner[key] = Some(value);

            None
        }
    }

    /// Removes all key-value pairs from the map.
    pub(crate) fn clear(&mut self) {
        self.inner.clear();
    }

    /// Returns an iterator over the map.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (usize, &V)> {
        self.inner
            .iter()
            .enumerate()
            .filter_map(|(i, v)| v.as_ref().map(|v| (i, v)))
    }

    /// Returns a mutable iterator over the map.
    pub(crate) fn iter_mut(&mut self) -> impl Iterator<Item = (usize, &mut V)> {
        self.inner
            .iter_mut()
            .enumerate()
            .filter_map(|(i, v)| v.as_mut().map(|v| (i, v)))
    }

    /// Retains only the elements specified by the predicate.
    ///
    /// In other words, remove all elements e for which f(&mut e) returns false.
    /// This method operates in place, visiting each element exactly once in the original order,
    /// and preserves the order of the retained elements.
    pub(crate) fn retain<F>(&mut self, f: F)
    where
        F: Fn(usize, &V) -> bool,
    {
        for (i, v) in self
            .inner
            .iter_mut()
            .enumerate()
            .filter(|(_, v)| v.is_some())
        {
            if !f(i, v.as_ref().unwrap()) {
                *v = None;
            }
        }
    }
}

impl<V: Clone> Clone for VecMap<V> {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
