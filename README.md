# futures-concurrency-dynamic

A dynamic merge combinator for Rust streams, based on [futures-concurrency](https://github.com/yoshuawuyts/futures-concurrency).

## Overview

This library provides `DynamicMerge`, a stream combinator that allows you to:

- Merge streams with different concrete types (but same `Item` type)
- Dynamically add new streams at runtime
- Automatically drop completed streams

## Relation to futures-concurrency

While `futures-concurrency` provides excellent combinators for fixed sets of futures and streams known at compile time, this library extends that concept to support dynamic stream collections where:

- Streams can be added during runtime
- Each stream can have a different concrete type
- The number of streams is not known at compile time
- Streams are automatically cleaned up when they complete

## Example

```rust
use futures_concurrency_dynamic_merge::DynamicMerge;
use futures_util::StreamExt;

#[tokio::main]
async fn main() {
    let mut merge = DynamicMerge::<i32>::new();

    // Add streams with different concrete types
    merge.push(futures_util::stream::iter(vec![1, 2, 3]));
    merge.push(futures_util::stream::once(async { 42 }));

    // Add more streams dynamically during iteration
    while let Some(value) = merge.next().await {
        println!("Got: {}", value);

        // Dynamically add new streams based on events
        if value == 42 {
            merge.push(futures_util::stream::iter(vec![100, 200]));
        }
    }

    // All streams are automatically cleaned up when done
    assert_eq!(merge.len(), 0);
}
```

## API

### Direct API

- `DynamicMerge::new()` - Create empty merge
- `DynamicMerge::with_capacity(n)` - Pre-allocate capacity
- `push(stream)` - Add a stream (can be called anytime)
- `len()` / `is_empty()` - Query active stream count
- `clear()` - Remove all streams
- `close()` - Signal no more streams will be added

### Handle-based API

For cases where you need separate mutable access to the stream and the ability to push new streams:

```rust
use futures_concurrency_dynamic::dynamic_merge_with_handle;
use futures_util::StreamExt;

#[tokio::main]
async fn main() {
    // Split into stream and handle components
    let (mut stream, mut handle) = dynamic_merge_with_handle::<i32>();

    // One task pushes streams
    let producer = tokio::spawn(async move {
        handle.push(futures_util::stream::iter(vec![1, 2, 3]));
        handle.push(futures_util::stream::iter(vec![4, 5, 6]));
        handle.close();
    });

    // Another task consumes from the stream
    let consumer = tokio::spawn(async move {
        while let Some(value) = stream.next().await {
            println!("Got: {}", value);
        }
    });

    producer.await.unwrap();
    consumer.await.unwrap();
}
```

**Handle API:**
- `dynamic_merge_with_handle()` - Create stream and handle pair
- `DynamicMergeHandle::push(stream)` - Add a stream via handle
- `DynamicMergeHandle::close()` - Signal completion via handle
- `DynamicMergeHandle::len()` / `is_empty()` - Query via handle
- `DynamicMergeHandle::clear()` - Clear all streams via handle
- `DynamicMergeHandle::clone()` - Clone the handle for shared access

## License

MIT OR Apache-2.0
