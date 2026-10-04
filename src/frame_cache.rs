pub struct FrameCache<K, V> {
    entries: Vec<Entry<K, V>>,
}

struct Entry<K, V> {
    key: K,
    value: V,
    used: bool,
}

impl<K, V> Default for FrameCache<K, V> {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<K: PartialEq, V> FrameCache<K, V> {
    pub fn get(&mut self, key: &K) -> Option<&V> {
        let entry = self.entries.iter_mut().find(|entry| entry.key == *key)?;
        entry.used = true;
        Some(&entry.value)
    }

    pub fn insert(&mut self, key: K, value: V) {
        self.entries.push(Entry {
            key,
            value,
            used: true,
        });
    }

    pub fn finish_frame(&mut self, mut release: impl FnMut(&V)) {
        self.entries.retain_mut(|entry| {
            if entry.used {
                entry.used = false;
                true
            } else {
                release(&entry.value);
                false
            }
        });
    }

    pub fn clear(&mut self, mut release: impl FnMut(V)) {
        for entry in self.entries.drain(..) {
            release(entry.value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FrameCache;

    #[test]
    fn reused_values_survive_and_unused_values_are_released_once() {
        let mut cache = FrameCache::default();
        let mut released = Vec::new();
        cache.insert("first", 1);
        cache.insert("second", 2);
        cache.finish_frame(|value| released.push(*value));
        assert_eq!(cache.get(&"first"), Some(&1));
        cache.finish_frame(|value| released.push(*value));
        assert_eq!(released, [2]);
        assert_eq!(cache.get(&"second"), None);
        cache.clear(|value| released.push(value));
        cache.clear(|value| released.push(value));
        assert_eq!(released, [2, 1]);
        assert_eq!(cache.get(&"first"), None);
    }
}
