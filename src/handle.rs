//! Handle-based API for DynamicMerge that allows separate mutable access.
//!
//! This module provides a way to split the dynamic merge into two components:
//! - A `DynamicMergeStream` that implements `Stream` and can be polled
//! - A `DynamicMergeHandle` that can push new streams
//!
//! This allows different parts of your code to hold mutable references to each independently.

use crate::DynamicMerge;
use futures_core::Stream;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

/// A handle for pushing new streams into a `DynamicMergeStream`.
///
/// This handle shares ownership of the underlying `DynamicMerge` with the
/// `DynamicMergeStream`, allowing you to push new streams while the stream is
/// being polled elsewhere.
///
/// # Examples
///
/// ```
/// use futures_concurrency_dynamic::dynamic_merge_with_handle;
/// use futures_util::stream::{self, StreamExt};
///
/// # async fn example() {
/// let (mut stream, mut handle) = dynamic_merge_with_handle::<i32>();
///
/// // Push streams via the handle
/// handle.push(stream::iter(vec![1, 2, 3]));
/// handle.push(stream::iter(vec![4, 5, 6]));
///
/// // Poll the stream elsewhere
/// let items: Vec<i32> = stream.collect().await;
/// # }
/// ```
pub struct DynamicMergeHandle<T> {
    shared: Arc<Mutex<DynamicMerge<T>>>,
}

impl<T> DynamicMergeHandle<T> {
    /// Pushes a new stream into the merge.
    ///
    /// The stream will be polled concurrently with other streams. When the stream
    /// produces an item, it will be returned from the merge. When the stream
    /// completes, it is automatically removed.
    ///
    /// # Examples
    ///
    /// ```
    /// use futures_concurrency_dynamic::dynamic_merge_with_handle;
    /// use futures_util::stream;
    ///
    /// let (stream, mut handle) = dynamic_merge_with_handle::<i32>();
    /// handle.push(stream::iter(vec![1, 2, 3]));
    /// handle.push(stream::iter(vec![4, 5, 6]));
    /// ```
    pub fn push<S>(&mut self, stream: S)
    where
        S: Stream<Item = T> + Send + 'static,
    {
        self.shared.lock().unwrap().push(stream);
    }

    /// Returns the number of active streams currently in the merge.
    ///
    /// # Examples
    ///
    /// ```
    /// use futures_concurrency_dynamic::dynamic_merge_with_handle;
    /// use futures_util::stream;
    ///
    /// let (stream, mut handle) = dynamic_merge_with_handle::<i32>();
    /// assert_eq!(handle.len(), 0);
    ///
    /// handle.push(stream::iter(vec![1, 2, 3]));
    /// assert_eq!(handle.len(), 1);
    /// ```
    pub fn len(&self) -> usize {
        self.shared.lock().unwrap().len()
    }

    /// Returns `true` if there are no active streams in the merge.
    ///
    /// # Examples
    ///
    /// ```
    /// use futures_concurrency_dynamic::dynamic_merge_with_handle;
    ///
    /// let (stream, handle) = dynamic_merge_with_handle::<i32>();
    /// assert!(handle.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.shared.lock().unwrap().is_empty()
    }

    /// Clears all streams from the merge, removing all active streams.
    ///
    /// # Examples
    ///
    /// ```
    /// use futures_concurrency_dynamic::dynamic_merge_with_handle;
    /// use futures_util::stream;
    ///
    /// let (stream, mut handle) = dynamic_merge_with_handle::<i32>();
    /// handle.push(stream::iter(vec![1, 2, 3]));
    /// assert_eq!(handle.len(), 1);
    ///
    /// handle.clear();
    /// assert_eq!(handle.len(), 0);
    /// ```
    pub fn clear(&mut self) {
        self.shared.lock().unwrap().clear();
    }
}

impl<T> Clone for DynamicMergeHandle<T> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

/// The stream component of a split dynamic merge.
///
/// This stream can be polled to receive items from all streams added via the
/// corresponding `DynamicMergeHandle`.
///
/// Created via [`dynamic_merge_with_handle`].
pub struct DynamicMergeStream<T> {
    shared: Arc<Mutex<DynamicMerge<T>>>,
}

impl<T> Stream for DynamicMergeStream<T> {
    type Item = T;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut shared = self.shared.lock().unwrap();
        Pin::new(&mut *shared).poll_next(cx)
    }
}

/// Creates a new dynamic merge with a separate handle for pushing streams.
///
/// Returns a tuple of `(stream, handle)` where:
/// - `stream` implements `Stream` and yields items from all added streams
/// - `handle` allows pushing new streams and controlling the merge lifecycle
///
/// This allows the stream and handle to be held by different parts of your code
/// with independent mutable access.
///
/// # Examples
///
/// ```
/// use futures_concurrency_dynamic::dynamic_merge_with_handle;
/// use futures_util::stream::{self, StreamExt};
///
/// # async fn example() {
/// let (mut stream, mut handle) = dynamic_merge_with_handle::<i32>();
///
/// // In one part of code: push streams
/// handle.push(stream::iter(vec![1, 2, 3]));
/// handle.push(stream::iter(vec![4, 5, 6]));
///
/// // In another part: consume the stream
/// while let Some(item) = stream.next().await {
///     println!("Got: {}", item);
/// }
/// # }
/// ```
///
/// # Type Parameters
///
/// - `T`: The item type that all streams must produce
///
/// Note: All streams must be `Send + 'static`. If you need to use borrowed streams,
/// use `DynamicMerge` directly instead of the handle-based API.
pub fn dynamic_merge_with_handle<T>() -> (DynamicMergeStream<T>, DynamicMergeHandle<T>) {
    let shared = Arc::new(Mutex::new(DynamicMerge::new()));
    let stream = DynamicMergeStream {
        shared: Arc::clone(&shared),
    };
    let handle = DynamicMergeHandle { shared };
    (stream, handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream::{self, StreamExt};

    #[tokio::test]
    async fn test_basic_handle_usage() {
        let (mut stream, mut handle) = dynamic_merge_with_handle::<i32>();

        handle.push(stream::iter(vec![1, 2, 3]));
        handle.push(stream::iter(vec![4, 5, 6]));

        let mut items = Vec::new();
        while let Some(item) = stream.next().await {
            items.push(item);
        }

        items.sort();
        assert_eq!(items, vec![1, 2, 3, 4, 5, 6]);
    }

    #[tokio::test]
    async fn test_separate_mutable_access() {
        let (mut stream, mut handle) = dynamic_merge_with_handle::<i32>();

        // Push both streams first
        handle.push(stream::iter(vec![1, 2, 3]));
        handle.push(stream::iter(vec![4, 5, 6]));

        // Then consume from the stream
        let mut items = Vec::new();
        while let Some(item) = stream.next().await {
            items.push(item);
        }

        items.sort();
        assert_eq!(items, vec![1, 2, 3, 4, 5, 6]);
    }

    #[tokio::test]
    async fn test_handle_clone() {
        let (mut stream, mut handle1) = dynamic_merge_with_handle::<i32>();
        let mut handle2 = handle1.clone();

        handle1.push(stream::iter(vec![1, 2, 3]));
        handle2.push(stream::iter(vec![4, 5, 6]));

        let mut items = Vec::new();
        while let Some(item) = stream.next().await {
            items.push(item);
        }

        items.sort();
        assert_eq!(items, vec![1, 2, 3, 4, 5, 6]);
    }

    #[test]
    fn test_handle_len_and_empty() {
        let (_stream, mut handle) = dynamic_merge_with_handle::<i32>();
        assert!(handle.is_empty());
        assert_eq!(handle.len(), 0);

        handle.push(stream::iter(vec![1, 2, 3]));
        assert!(!handle.is_empty());
        assert_eq!(handle.len(), 1);

        handle.push(stream::iter(vec![4, 5, 6]));
        assert_eq!(handle.len(), 2);
    }

    #[test]
    fn test_handle_clear() {
        let (_stream, mut handle) = dynamic_merge_with_handle::<i32>();

        handle.push(stream::iter(vec![1, 2, 3]));
        handle.push(stream::iter(vec![4, 5, 6]));
        assert_eq!(handle.len(), 2);

        handle.clear();
        assert_eq!(handle.len(), 0);
        assert!(handle.is_empty());
    }

    #[tokio::test]
    async fn test_dynamic_push_while_polling() {
        let (mut stream, mut handle) = dynamic_merge_with_handle::<i32>();

        handle.push(stream::iter(vec![1, 2]));

        let item1 = stream.next().await;
        assert!(item1.is_some());

        // Push more while already polling
        handle.push(stream::iter(vec![3, 4]));

        let item2 = stream.next().await;
        assert!(item2.is_some());

        let item3 = stream.next().await;
        assert!(item3.is_some());

        let item4 = stream.next().await;
        assert!(item4.is_some());
    }
}
