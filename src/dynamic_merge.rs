//! Dynamic merge combinator using SelectAll from futures-util.
//!
//! This module provides a wrapper around `SelectAll` that handles the boxing internally
//! for a more ergonomic API.

use futures_core::Stream;
use futures_util::stream::SelectAll;
use pin_project::pin_project;
use std::pin::Pin;
use std::task::{Context, Poll};

/// A dynamically mergeable collection of streams built on top of SelectAll.
///
/// This wraps `futures_util::stream::SelectAll` and handles the boxing of streams
/// internally to provide a more ergonomic API.
///
/// # Examples
///
/// ```
/// use futures_concurrency_dynamic::DynamicMerge;
/// use futures_util::stream::{self, StreamExt};
///
/// # async fn example() {
/// let mut merge = DynamicMerge::new();
/// merge.push(stream::iter(vec![1, 2, 3]));
/// merge.push(stream::iter(vec![4, 5, 6]));
///
/// let items: Vec<i32> = merge.collect().await;
/// // items will contain [1, 2, 3, 4, 5, 6] in some order
/// # }
/// ```
#[pin_project]
pub struct DynamicMerge<T> {
    #[pin]
    select_all: SelectAll<Pin<Box<dyn Stream<Item = T> + Send>>>,
}

impl<T> DynamicMerge<T> {
    /// Creates a new empty `DynamicMerge`.
    ///
    /// # Examples
    ///
    /// ```
    /// use futures_concurrency_dynamic::DynamicMerge;
    ///
    /// let merge = DynamicMerge::<i32>::new();
    /// ```
    pub fn new() -> Self {
        Self {
            select_all: SelectAll::new(),
        }
    }

    /// Creates a new `DynamicMerge` with the specified capacity.
    ///
    /// Note: `SelectAll` doesn't support pre-allocation, so this is equivalent to `new()`.
    ///
    /// # Examples
    ///
    /// ```
    /// use futures_concurrency_dynamic::DynamicMerge;
    ///
    /// let merge = DynamicMerge::<i32>::with_capacity(10);
    /// ```
    pub fn with_capacity(_capacity: usize) -> Self {
        Self::new()
    }

    /// Pushes a new stream into the merge.
    ///
    /// The stream will be polled concurrently with other streams. When the stream
    /// produces an item, it will be returned from the merge. When the stream
    /// completes, it is automatically removed.
    ///
    /// # Examples
    ///
    /// ```
    /// use futures_concurrency_dynamic::DynamicMerge;
    /// use futures_util::stream;
    ///
    /// let mut merge = DynamicMerge::new();
    /// merge.push(stream::iter(vec![1, 2, 3]));
    /// merge.push(stream::iter(vec![4, 5, 6]));
    /// ```
    pub fn push<S>(&mut self, stream: S)
    where
        S: Stream<Item = T> + Send + 'static,
    {
        self.select_all.push(Box::pin(stream));
    }

    /// Returns the number of active streams currently in the merge.
    ///
    /// # Examples
    ///
    /// ```
    /// use futures_concurrency_dynamic::DynamicMerge;
    /// use futures_util::stream;
    ///
    /// let mut merge = DynamicMerge::new();
    /// assert_eq!(merge.len(), 0);
    ///
    /// merge.push(stream::iter(vec![1, 2, 3]));
    /// assert_eq!(merge.len(), 1);
    /// ```
    pub fn len(&self) -> usize {
        self.select_all.len()
    }

    /// Returns `true` if there are no active streams in the merge.
    ///
    /// # Examples
    ///
    /// ```
    /// use futures_concurrency_dynamic::DynamicMerge;
    ///
    /// let merge = DynamicMerge::<i32>::new();
    /// assert!(merge.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.select_all.is_empty()
    }

    /// Clears all streams from the merge, removing all active streams.
    ///
    /// # Examples
    ///
    /// ```
    /// use futures_concurrency_dynamic::DynamicMerge;
    /// use futures_util::stream;
    ///
    /// let mut merge = DynamicMerge::new();
    /// merge.push(stream::iter(vec![1, 2, 3]));
    /// merge.push(stream::iter(vec![4, 5, 6]));
    /// assert_eq!(merge.len(), 2);
    ///
    /// merge.clear();
    /// assert_eq!(merge.len(), 0);
    /// ```
    pub fn clear(&mut self) {
        self.select_all.clear();
    }
}

impl<T> Default for DynamicMerge<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Stream for DynamicMerge<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.project();
        this.select_all.poll_next(cx)
    }
}

impl<T> From<Vec<Pin<Box<dyn Stream<Item = T> + Send>>>> for DynamicMerge<T> {
    fn from(streams: Vec<Pin<Box<dyn Stream<Item = T> + Send>>>) -> Self {
        Self {
            select_all: SelectAll::from_iter(streams),
        }
    }
}

/// Extension trait for creating a `DynamicMerge` from streams.
pub trait IntoDynamicMerge<T> {
    /// Creates a `DynamicMerge` from this iterator of streams.
    fn into_dynamic_merge(self) -> DynamicMerge<T>;
}

impl<I, S, T> IntoDynamicMerge<T> for I
where
    I: IntoIterator<Item = S>,
    S: Stream<Item = T> + Send + 'static,
{
    fn into_dynamic_merge(self) -> DynamicMerge<T> {
        let streams: Vec<Pin<Box<dyn Stream<Item = T> + Send>>> = self
            .into_iter()
            .map(|s| Box::pin(s) as Pin<Box<dyn Stream<Item = T> + Send>>)
            .collect();

        DynamicMerge::from(streams)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream::{self, StreamExt};

    #[test]
    fn test_empty_merge() {
        let merge = DynamicMerge::<i32>::new();
        assert!(merge.is_empty());
    }

    #[test]
    fn test_push_stream() {
        let mut merge = DynamicMerge::new();
        merge.push(stream::iter(vec![1, 2, 3]));
        assert_eq!(merge.len(), 1);
    }

    #[test]
    fn test_clear() {
        let mut merge = DynamicMerge::new();
        merge.push(stream::iter(vec![1, 2, 3]));
        merge.push(stream::iter(vec![4, 5, 6]));
        assert_eq!(merge.len(), 2);

        merge.clear();
        assert_eq!(merge.len(), 0);
        assert!(merge.is_empty());
    }

    #[tokio::test]
    async fn test_basic_consumption() {
        let mut merge = DynamicMerge::new();
        merge.push(stream::iter(vec![1, 2, 3]));
        merge.push(stream::iter(vec![4, 5, 6]));

        let mut items = Vec::new();
        while let Some(item) = merge.next().await {
            items.push(item);
        }

        items.sort();
        assert_eq!(items, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_from_vec() {
        let streams: Vec<Pin<Box<dyn Stream<Item = i32> + Send>>> = vec![
            Box::pin(stream::iter(vec![1, 2, 3])),
            Box::pin(stream::iter(vec![4, 5, 6])),
        ];

        let merge = DynamicMerge::from(streams);
        assert_eq!(merge.len(), 2);
    }

    #[test]
    fn test_into_dynamic_merge() {
        let streams = vec![stream::iter(vec![1, 2, 3]), stream::iter(vec![4, 5, 6])];

        let merge = streams.into_dynamic_merge();
        assert_eq!(merge.len(), 2);
    }
}
