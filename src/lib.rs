//! Dynamic merge combinator for streams with different types.
//!
//! Streams with different concrete types but the same Item type can be merged together.
//! Completed streams are automatically dropped. Each stream has a custom waker for efficient polling.

use futures_core::Stream;
use pin_project::pin_project;
use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Wake, Waker};

/// Inner shared state for the DynamicMerge.
struct Inner {
    /// Queue of stream indices that are ready to be polled.
    ready_queue: VecDeque<usize>,
    /// The parent waker to wake when streams become ready.
    parent_waker: Option<Waker>,
}

impl Inner {
    fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
            parent_waker: None,
        }
    }

    fn mark_ready(&mut self, index: usize) {
        // Only add if not already in queue
        if !self.ready_queue.contains(&index) {
            self.ready_queue.push_back(index);
        }

        // Wake the parent
        if let Some(waker) = &self.parent_waker {
            waker.wake_by_ref();
        }
    }

    fn set_parent_waker(&mut self, waker: &Waker) {
        // Only update if different
        if self
            .parent_waker
            .as_ref()
            .map_or(true, |w| !w.will_wake(waker))
        {
            self.parent_waker = Some(waker.clone());
        }
    }
}

/// Custom waker that marks a specific stream as ready.
struct StreamWaker {
    index: usize,
    inner: Arc<Mutex<Inner>>,
}

impl Wake for StreamWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.inner.lock().unwrap().mark_ready(self.index);
    }
}

/// A stream wrapper that holds the stream and its waker.
struct StreamEntry<'a, T> {
    stream: Pin<Box<dyn Stream<Item = T> + Send + 'a>>,
    waker: Waker,
}

/// A dynamically mergeable collection of streams.
///
/// Streams must produce the same `Item` type but can have different concrete types.
/// Completed streams are removed automatically. Custom wakers ensure only ready streams are polled.
#[pin_project]
pub struct DynamicMerge<'a, T> {
    streams: Vec<Option<StreamEntry<'a, T>>>,
    inner: Arc<Mutex<Inner>>,
    next_index: usize,
}

impl<'a, T> DynamicMerge<'a, T> {
    /// Creates a new empty `DynamicMerge`.
    pub fn new() -> Self {
        Self {
            streams: Vec::new(),
            inner: Arc::new(Mutex::new(Inner::new())),
            next_index: 0,
        }
    }

    /// Creates a new `DynamicMerge` with the specified capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            streams: Vec::with_capacity(capacity),
            inner: Arc::new(Mutex::new(Inner::new())),
            next_index: 0,
        }
    }

    /// Adds a new stream to the merge.
    pub fn push<S>(&mut self, stream: S)
    where
        S: Stream<Item = T> + Send + 'a,
    {
        let index = self.find_empty_slot();

        // Create a custom waker for this stream
        let stream_waker = Arc::new(StreamWaker {
            index,
            inner: Arc::clone(&self.inner),
        });
        let waker = Waker::from(stream_waker);

        let entry = StreamEntry {
            stream: Box::pin(stream),
            waker,
        };

        if index < self.streams.len() {
            self.streams[index] = Some(entry);
        } else {
            self.streams.push(Some(entry));
        }

        // Mark this stream as ready to be polled
        self.inner.lock().unwrap().mark_ready(index);
    }

    /// Find an empty slot or return the next index
    fn find_empty_slot(&mut self) -> usize {
        for (i, slot) in self.streams.iter().enumerate() {
            if slot.is_none() {
                return i;
            }
        }
        self.streams.len()
    }

    /// Returns the number of active streams.
    pub fn len(&self) -> usize {
        self.streams.iter().filter(|s| s.is_some()).count()
    }

    /// Returns `true` if there are no active streams.
    pub fn is_empty(&self) -> bool {
        self.streams.iter().all(|s| s.is_none())
    }

    /// Clears all streams from the merge.
    pub fn clear(&mut self) {
        self.streams.clear();
        self.inner.lock().unwrap().ready_queue.clear();
    }
}

impl<'a, T> Default for DynamicMerge<'a, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a, T> Stream for DynamicMerge<'a, T> {
    type Item = T;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Update the parent waker
        self.inner.lock().unwrap().set_parent_waker(cx.waker());

        // If no streams, we're done
        if self.is_empty() {
            return Poll::Ready(None);
        }

        loop {
            // Get the next ready stream index
            let index = {
                let mut inner = self.inner.lock().unwrap();
                inner.ready_queue.pop_front()
            };

            let Some(index) = index else {
                // No ready streams, return Pending
                return Poll::Pending;
            };

            // Check if this stream still exists
            let Some(entry) = &mut self.streams.get_mut(index).and_then(|s| s.as_mut()) else {
                // Stream was removed, continue to next ready stream
                continue;
            };

            // Create a context with the stream's custom waker
            let stream_waker = entry.waker.clone();
            let mut stream_cx = Context::from_waker(&stream_waker);

            // Poll the stream
            match entry.stream.as_mut().poll_next(&mut stream_cx) {
                Poll::Ready(Some(item)) => {
                    // Stream produced an item
                    // Mark it as ready again in case it has more items
                    self.inner.lock().unwrap().mark_ready(index);
                    return Poll::Ready(Some(item));
                }
                Poll::Ready(None) => {
                    // Stream is exhausted, remove it
                    self.streams[index] = None;

                    // If all streams are done, return None
                    if self.is_empty() {
                        return Poll::Ready(None);
                    }
                    // Otherwise continue to next ready stream
                    continue;
                }
                Poll::Pending => {
                    // Stream is not ready, it will wake us later
                    // Continue to next ready stream
                    continue;
                }
            }
        }
    }
}

impl<'a, T: 'a> From<Vec<Pin<Box<dyn Stream<Item = T> + Send + 'a>>>> for DynamicMerge<'a, T> {
    fn from(streams: Vec<Pin<Box<dyn Stream<Item = T> + Send + 'a>>>) -> Self {
        let mut merge = DynamicMerge::with_capacity(streams.len());
        for stream in streams {
            // This is a bit inefficient but keeps the API simple
            // We box the already-boxed stream
            struct BoxedStream<'a, T>(Pin<Box<dyn Stream<Item = T> + Send + 'a>>);

            impl<'a, T> Stream for BoxedStream<'a, T> {
                type Item = T;

                fn poll_next(
                    mut self: Pin<&mut Self>,
                    cx: &mut Context<'_>,
                ) -> Poll<Option<Self::Item>> {
                    self.0.as_mut().poll_next(cx)
                }
            }

            merge.push(BoxedStream(stream));
        }
        merge
    }
}

/// Extension trait for creating a `DynamicMerge` from streams.
pub trait IntoDynamicMerge<'a>: Sized {
    type Item;

    /// Converts into a `DynamicMerge`.
    fn into_dynamic_merge(self) -> DynamicMerge<'a, Self::Item>;
}

impl<'a, T, I, S> IntoDynamicMerge<'a> for I
where
    I: IntoIterator<Item = S>,
    S: Stream<Item = T> + Send + 'a,
{
    type Item = T;

    fn into_dynamic_merge(self) -> DynamicMerge<'a, T> {
        let mut merge = DynamicMerge::new();
        for stream in self {
            merge.push(stream);
        }
        merge
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::task::{Context, Poll};

    struct ManualStream {
        items: Vec<i32>,
        index: usize,
    }

    impl ManualStream {
        fn new(items: Vec<i32>) -> Self {
            Self { items, index: 0 }
        }
    }

    impl Stream for ManualStream {
        type Item = i32;

        fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            if self.index < self.items.len() {
                let item = self.items[self.index];
                self.index += 1;
                Poll::Ready(Some(item))
            } else {
                Poll::Ready(None)
            }
        }
    }

    #[test]
    fn test_empty_merge() {
        let merge = DynamicMerge::<i32>::new();
        assert_eq!(merge.len(), 0);
        assert!(merge.is_empty());
    }

    #[test]
    fn test_push_stream() {
        let mut merge = DynamicMerge::<i32>::new();
        merge.push(ManualStream::new(vec![1, 2, 3]));
        assert_eq!(merge.len(), 1);
        assert!(!merge.is_empty());
    }

    #[test]
    fn test_with_capacity() {
        let merge = DynamicMerge::<i32>::with_capacity(10);
        assert_eq!(merge.len(), 0);
        assert_eq!(merge.streams.capacity(), 10);
    }

    #[test]
    fn test_clear() {
        let mut merge = DynamicMerge::<i32>::new();
        merge.push(ManualStream::new(vec![1, 2, 3]));
        merge.push(ManualStream::new(vec![4, 5, 6]));
        assert_eq!(merge.len(), 2);

        merge.clear();
        assert_eq!(merge.len(), 0);
        assert!(merge.is_empty());
    }

    #[test]
    fn test_ready_queue() {
        let mut merge = DynamicMerge::<i32>::new();

        // Add a stream
        merge.push(ManualStream::new(vec![1, 2]));

        // Should have one ready stream
        assert_eq!(merge.inner.lock().unwrap().ready_queue.len(), 1);
    }

    #[test]
    fn test_reuses_slots() {
        let mut merge = DynamicMerge::<i32>::new();

        merge.push(ManualStream::new(vec![1]));
        merge.push(ManualStream::new(vec![2]));
        merge.push(ManualStream::new(vec![3]));

        assert_eq!(merge.len(), 3);
        assert_eq!(merge.streams.len(), 3);

        // Manually remove a stream in the middle
        merge.streams[1] = None;

        // Add a new stream - should reuse slot 1
        merge.push(ManualStream::new(vec![4]));

        assert_eq!(merge.len(), 3);
        assert_eq!(merge.streams.len(), 3); // Should not grow
    }

    #[tokio::test]
    async fn test_async_basic_consumption() {
        use futures_util::StreamExt;

        let mut merge = DynamicMerge::<i32>::new();

        merge.push(ManualStream::new(vec![1, 2, 3]));
        merge.push(ManualStream::new(vec![10, 20]));
        merge.push(ManualStream::new(vec![100, 200, 300, 400]));

        let mut results = Vec::new();
        while let Some(value) = merge.next().await {
            results.push(value);
        }

        // All items should be collected
        assert_eq!(results.len(), 9);
        assert!(results.contains(&1));
        assert!(results.contains(&2));
        assert!(results.contains(&3));
        assert!(results.contains(&10));
        assert!(results.contains(&20));
        assert!(results.contains(&100));
        assert!(results.contains(&200));
        assert!(results.contains(&300));
        assert!(results.contains(&400));

        // All streams should be cleaned up
        assert_eq!(merge.len(), 0);
        assert!(merge.is_empty());
    }

    #[tokio::test]
    async fn test_dynamic_stream_insertion() {
        use futures_util::StreamExt;

        let mut merge = DynamicMerge::<i32>::new();

        // Start with one stream
        merge.push(ManualStream::new(vec![1, 2]));

        let mut results = Vec::new();

        // Get first item
        if let Some(value) = merge.next().await {
            results.push(value);
        }

        // Add more streams while processing
        merge.push(ManualStream::new(vec![10, 20, 30]));
        merge.push(ManualStream::new(vec![100]));

        // Continue collecting
        while let Some(value) = merge.next().await {
            results.push(value);

            // Add another stream mid-processing when we see value 10
            if value == 10 {
                merge.push(ManualStream::new(vec![1000, 2000]));
            }
        }

        // Should have collected all items
        assert_eq!(results.len(), 8); // 2 + 3 + 1 + 2
        assert!(results.contains(&1));
        assert!(results.contains(&2));
        assert!(results.contains(&10));
        assert!(results.contains(&20));
        assert!(results.contains(&30));
        assert!(results.contains(&100));
        assert!(results.contains(&1000));
        assert!(results.contains(&2000));

        // All streams should be cleaned up
        assert_eq!(merge.len(), 0);
    }

    #[tokio::test]
    async fn test_streams_auto_removed() {
        use futures_util::StreamExt;

        let mut merge = DynamicMerge::<i32>::new();

        // Add streams with different lengths
        merge.push(ManualStream::new(vec![1])); // Length 1
        merge.push(ManualStream::new(vec![2, 3, 4])); // Length 3
        merge.push(ManualStream::new(vec![5, 6])); // Length 2

        assert_eq!(merge.len(), 3);

        let mut count = 0;
        while let Some(_) = merge.next().await {
            count += 1;

            // Check that completed streams are being removed
            // After 5 item, at least one stream are removed
            if count == 5 {
                assert_ne!(merge.len(), 3);
            }
        }
    }

    #[tokio::test]
    async fn test_interleaved_polling() {
        use futures_util::StreamExt;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc as StdArc;

        // Track poll counts
        let poll_count = StdArc::new(AtomicUsize::new(0));

        struct PollCountStream {
            items: Vec<i32>,
            index: usize,
            counter: StdArc<AtomicUsize>,
        }

        impl Stream for PollCountStream {
            type Item = i32;

            fn poll_next(
                mut self: Pin<&mut Self>,
                _cx: &mut Context<'_>,
            ) -> Poll<Option<Self::Item>> {
                self.counter.fetch_add(1, Ordering::SeqCst);

                if self.index < self.items.len() {
                    let item = self.items[self.index];
                    self.index += 1;
                    Poll::Ready(Some(item))
                } else {
                    Poll::Ready(None)
                }
            }
        }

        let mut merge = DynamicMerge::<i32>::new();

        merge.push(PollCountStream {
            items: vec![1, 2],
            index: 0,
            counter: StdArc::clone(&poll_count),
        });
        merge.push(PollCountStream {
            items: vec![3, 4, 5],
            index: 0,
            counter: StdArc::clone(&poll_count),
        });

        let mut results = Vec::new();
        while let Some(value) = merge.next().await {
            results.push(value);
        }

        assert_eq!(results.len(), 5);

        // With custom wakers, we should have polled efficiently
        // Each stream polled once per item + once for completion
        // Stream 1: 3 polls (2 items + 1 done)
        // Stream 2: 4 polls (3 items + 1 done)
        // Total: 7 polls
        let total_polls = poll_count.load(Ordering::SeqCst);
        assert_eq!(total_polls, 7);
    }

    #[tokio::test]
    async fn test_non_static_streams() {
        use futures_util::StreamExt;

        // Data that streams will borrow
        let data = vec![1, 2, 3, 4, 5];
        let more_data = vec![10, 20, 30];

        let mut merge = DynamicMerge::new();

        // Create streams that borrow from local data
        let stream1 = futures_util::stream::iter(data.iter().copied());
        let stream2 = futures_util::stream::iter(more_data.iter().copied());

        merge.push(stream1);
        merge.push(stream2);

        let mut collected = Vec::new();
        while let Some(value) = merge.next().await {
            collected.push(value);
        }

        assert_eq!(collected.len(), 8);
        assert!(collected.contains(&1));
        assert!(collected.contains(&5));
        assert!(collected.contains(&10));
        assert!(collected.contains(&30));
    }

    #[tokio::test]
    async fn test_static_and_non_static_mixed() {
        use futures_util::StreamExt;

        let borrowed_data = vec![100, 200];

        let mut merge = DynamicMerge::new();

        // Non-static stream (borrows)
        merge.push(futures_util::stream::iter(borrowed_data.iter().copied()));

        // Static stream (owned)
        merge.push(futures_util::stream::iter(vec![1, 2, 3]));

        let collected: Vec<i32> = merge.collect().await;

        assert_eq!(collected.len(), 5);
        assert!(collected.contains(&1));
        assert!(collected.contains(&100));
    }
}
