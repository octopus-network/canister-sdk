use std::marker::PhantomData;
use std::mem::size_of;
use std::rc::Rc;

use candid::{CandidType, Deserialize};

use super::error::Result;
use super::{
    from_bytes, to_byte_vec, Memory, RestrictedMemory, StableBTreeMap, StableMemory, VirtualMemory, PADDING
};

type Mem<const INDEX: u8> = VirtualMemory<Rc<RestrictedMemory<StableMemory>>, INDEX>;

/// Inserting the same value twice will simply replace the inner value.
/// ```
/// use ic_stable_storage::StableLog;
/// let log = StableLog::<u64, 0>::from(vec![1, 2, 3]);
/// for val in &log {
/// // ...
/// }
/// ```
pub struct StableLog<T, const INDEX: u8> {
    _p: PhantomData<T>,
    inner: StableBTreeMap<Mem<INDEX>, Vec<u8>, Vec<u8>>,
}

impl<T, const INDEX: u8> Default for StableLog<T, INDEX> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const INDEX: u8> StableLog<T, INDEX> {
    const MAX_KEY_SIZE: u32 = size_of::<T>() as u32 + PADDING;
    const MAX_VALUE_SIZE: u32 = 0;

    /// Create a new instance of a [`StableLog`].
    pub fn new() -> Self {
        let inner = crate::MEM.with(|memory| {
            let virt_memory = VirtualMemory::<_, INDEX>::init(memory.clone());
            StableBTreeMap::init(virt_memory, Self::MAX_KEY_SIZE, Self::MAX_VALUE_SIZE)
        });

        Self {
            _p: PhantomData,
            inner,
        }
    }

    /// Total count of values.
    /// ```
    /// # use ic_stable_storage::StableLog;
    /// let mut log = StableLog::<u64, 0>::from(vec![1, 2]);
    /// assert_eq!(log.len(), 2);
    /// ```
    pub fn len(&self) -> u64 {
        self.inner.len()
    }

    /// Check if the `Log` is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T, const INDEX: u8> StableLog<T, INDEX>
where
    for<'de> T: CandidType + Deserialize<'de> + Copy,
{
    /// Push a new value to the end of the log.
    pub fn push(&mut self, val: T) -> Result<()> {
        let bytes = to_byte_vec(&val)?;
        self.inner.insert(bytes, vec![])?;
        Ok(())
    }

    /// Remove the first entry in the `Log`
    /// ```
    /// # use ic_stable_storage::StableLog;
    /// let mut log = StableLog::<u64, 0>::from(vec![1, 2]);
    /// assert_eq!(log.pop_front(), Some(1));
    /// ```
    pub fn pop_front(&mut self) -> Option<T> {
        let (entry, _) = self.inner.iter().next()?;
        self.inner.remove(&entry)?;
        from_bytes(&entry).ok()
    }

    /// Remove the last entry in the `Log`
    /// ```
    /// # use ic_stable_storage::StableLog;
    /// let mut log = StableLog::<u64, 0>::from(vec![1, 2]);
    /// assert_eq!(log.pop_back(), Some(2));
    /// ```
    pub fn pop_back(&mut self) -> Option<T> {
        let (entry, _) = self.inner.iter().last()?;
        self.inner.remove(&entry)?;
        from_bytes(&entry).ok()
    }

    /// Convert the [`Log<T>`] into a `Vec<T>`.
    /// This would load and deserialize every value in the `Log` which could be an expensive
    /// operation if there are a lot of values.
    /// ```
    /// # use ic_stable_storage::StableLog;
    /// let mut log = StableLog::<u64, 0>::from(vec![1, 2]);
    /// assert_eq!(log.to_vec(), vec![1, 2]);
    /// ```
    pub fn to_vec(self) -> Vec<T> {
        self.into_iter().collect()
    }
}

impl<T, const INDEX: u8> From<Vec<T>> for StableLog<T, INDEX>
where
    for<'de> T: CandidType + Deserialize<'de> + Copy,
{
    fn from(v: Vec<T>) -> Self {
        let mut log = StableLog::new();
        let _ = v.into_iter().try_for_each(|val| log.push(val));
        log
    }
}

pub struct Iter<'a, T, M: Memory> {
    inner: super::Iter<'a, M, Vec<u8>, Vec<u8>>,
    _p: std::marker::PhantomData<T>,
}

impl<'a, T, M: Memory + Clone> Iterator for Iter<'a, T, M>
where
    for<'de> T: CandidType + Deserialize<'de>,
{
    type Item = T;

    fn next(&mut self) -> Option<T> {
        self.inner.next().and_then(|(k, _)| from_bytes(&k).ok())
    }
}

impl<'a, T, const INDEX: u8> IntoIterator for &'a StableLog<T, INDEX>
where
    for<'de> T: CandidType + Deserialize<'de>,
{
    type Item = T;
    type IntoIter = Iter<'a, T, Mem<INDEX>>;

    fn into_iter(self) -> Self::IntoIter {
        Iter {
            inner: self.inner.iter(),
            _p: std::marker::PhantomData,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn push() {
        let mut log = StableLog::<u64, 0>::new();
        let _ = log.push(1);
        let _ = log.push(2);

        let expected = vec![1, 2];
        assert_eq!(log.to_vec(), expected);
    }

    #[test]
    fn pop_front_not_empty() {
        let mut log = StableLog::<u64, 0>::from(vec![1, 2]);
        assert_eq!(log.pop_front(), Some(1));
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn pop_front_empty() {
        let mut log = StableLog::<u64, 0>::new();
        assert!(log.pop_front().is_none());
    }

    #[test]
    fn pop_back_not_empty() {
        let mut log = StableLog::<u64, 0>::from(vec![1, 2, 3]);
        assert_eq!(log.pop_back(), Some(3));
        assert_eq!(log.len(), 2);
    }

    #[test]
    fn pop_back_empty() {
        let mut log = StableLog::<u64, 0>::new();
        assert!(log.pop_back().is_none());
    }

    #[test]
    fn iterator() {
        let log = StableLog::<u64, 0>::from(vec![1, 2]);
        let mut iter = log.into_iter();
        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.next(), Some(2));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn multiple_logs() {
        let log_1 = StableLog::<u64, 0>::from(vec![1, 2]);
        let log_2 = StableLog::<u64, 1>::from(vec![2, 3]);

        let mut iter = log_1.into_iter();
        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.next(), Some(2));
        assert_eq!(iter.next(), None);

        let mut iter = log_2.into_iter();
        assert_eq!(iter.next(), Some(2));
        assert_eq!(iter.next(), Some(3));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn insert_same_twice() {
        let log = StableLog::<u64, 0>::from(vec![1, 1]);
        assert_eq!(log.len(), 1);
    }
}
