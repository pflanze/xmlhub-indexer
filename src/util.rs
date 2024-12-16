use std::{
    collections::{BTreeMap, HashMap, HashSet},
    hash::Hash,
};

pub trait InsertValue<K, V> {
    /// Returns whether the value was newly added.
    fn insert_value(&mut self, key: K, val: V) -> bool;
}

impl<K: Hash + PartialEq + Eq + Clone, V: Hash + PartialEq + Eq> InsertValue<K, V>
    for HashMap<K, HashSet<V>>
{
    fn insert_value(&mut self, key: K, val: V) -> bool {
        if let Some(vals) = self.get_mut(&key) {
            vals.insert(val)
        } else {
            let mut vals = HashSet::new();
            vals.insert(val);
            self.insert(key.clone(), vals);
            true
        }
    }
}

impl<K: Ord + PartialEq + Eq + Clone, V: Hash + PartialEq + Eq> InsertValue<K, V>
    for BTreeMap<K, HashSet<V>>
{
    fn insert_value(&mut self, key: K, val: V) -> bool {
        if let Some(vals) = self.get_mut(&key) {
            vals.insert(val)
        } else {
            let mut vals = HashSet::new();
            vals.insert(val);
            self.insert(key.clone(), vals);
            true
        }
    }
}

/// Try to get the value for a key in a list of (key, value) pairings.
pub fn get_by_key<'t, K: Eq, T>(
    vals: &'t [T],
    get_key: impl Fn(&T) -> &K,
    key: &K,
) -> Option<&'t T> {
    vals.iter().find(|item| get_key(item) == key)
}

/// Create a new vector that contains copies of the elements of both.
pub fn append<T: Clone>(a: &[T], b: &[T]) -> Vec<T> {
    let mut vec = Vec::new();
    for v in a {
        vec.push(v.clone());
    }
    for v in b {
        vec.push(v.clone());
    }
    vec
}

/// Create a new vector that contains copies of the elements of all
/// vectors/slices. In other words, an n-ary `append`.
pub fn flatten<T: Clone, V1: AsRef<[T]>, V0: AsRef<[V1]>>(vectors: V0) -> Vec<T> {
    let mut result = Vec::new();
    for vec in vectors.as_ref() {
        for v in vec.as_ref() {
            result.push(v.clone());
        }
    }
    result
}

/// Replace groups of whitespace characters with a single space each.
pub fn normalize_whitespace(s: &str) -> String {
    let mut result = String::new();
    let mut last_was_whitespace = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if last_was_whitespace {
                ()
            } else {
                result.push(' ');
                last_was_whitespace = true;
            }
        } else {
            result.push(c);
            last_was_whitespace = false;
        }
    }
    result
}

#[cfg(test)]
#[test]
fn t_normalize_whitespace() {
    let t = normalize_whitespace;
    assert_eq!(t("Hi !"), "Hi !");
    assert_eq!(t(""), "");
    assert_eq!(t("Hi  !"), "Hi !");
    assert_eq!(t("  Hi  !\n\n\n"), " Hi ! ");
}
