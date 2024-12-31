use std::{
    collections::{BTreeMap, HashMap, HashSet},
    hash::Hash,
};

pub trait InsertValue<K, V> {
    /// Insert a value into a collection of value that `key` maps to,
    /// creating the collection and the mapping from key if it doesn't
    /// exist yet. Returns whether the value was newly added (false
    /// means `val` was already in there).
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
        // copy-paste
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

/// From a list of values, try to get the one for which an extracted
/// value matches `key`.
pub fn list_get_by_key<'t, K: Eq, T>(
    vals: &'t [T],
    get_key: impl Fn(&T) -> &K,
    key: &K,
) -> Option<&'t T> {
    vals.iter().find(|item| get_key(item) == key)
}

/// Create a new vector that contains copies of the elements of both
/// argument vectors or slices.
pub fn append<T, V1, V2>(a: V1, b: V2) -> Vec<T>
where
    V1: IntoIterator<Item = T>,
    V2: IntoIterator<Item = T>,
{
    let mut vec = Vec::new();
    for v in a {
        vec.push(v);
    }
    for v in b {
        vec.push(v);
    }
    vec
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

/// Convert a slice of references to a vector that owns the owned
/// versions of the items.
pub fn to_owned_items<O, T: ToOwned<Owned = O> + ?Sized>(vals: &[&T]) -> Vec<O> {
    vals.iter().map(|s| (*s).to_owned()).collect::<Vec<O>>()
}
