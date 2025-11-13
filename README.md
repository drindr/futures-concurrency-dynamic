# futures-concurrency-dynamic

A thin wrapper around `futures_util::stream::SelectAll` with ergonomic improvements.

## Overview

`DynamicMerge<T>` is essentially a type alias for `SelectAll<Pin<Box<dyn Stream<Item = T> + Send>>>` with convenience methods that handle the boxing internally. If you're familiar with `SelectAll`, you already know how to use this.

### Handle-based API

When you need separate mutable access:

```rust
use futures_concurrency_dynamic::dynamic_merge_with_handle;
use futures_util::stream::{self, StreamExt};

#[tokio::main]
async fn main() {
    let (mut stream, mut handle) = dynamic_merge_with_handle::<i32>();

    // Producer task
    tokio::spawn(async move {
        handle.push(stream::iter(vec![1, 2, 3]));
        handle.push(stream::iter(vec![4, 5, 6]));
    });

    // Consumer
    while let Some(item) = stream.next().await {
        println!("Got: {}", item);
    }
}
```

## API

- `DynamicMerge::new()` - Create a new merge
- `push(stream)` - Add a stream 
- `len()` / `is_empty()` - Query active streams
- `clear()` - Remove all streams
- `dynamic_merge_with_handle()` - Get separate handle and stream

## License

MIT OR Apache-2.0