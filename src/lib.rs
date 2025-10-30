//! Dynamic merge combinator for heterogeneous streams.
//!
//! This crate provides two ways to use dynamic stream merging:
//!
//! ## 1. Direct API with `DynamicMerge`
//!
//! Use `DynamicMerge` directly when you want a single object that owns both the stream
//! and the ability to push new streams:
//!
//! ```rust
//! use futures_concurrency_dynamic::DynamicMerge;
//! use futures_util::stream::{self, StreamExt};
//!
//! # async fn example() {
//! let mut merge = DynamicMerge::new();
//! merge.push(stream::iter(vec![1, 2, 3]));
//! merge.push(stream::iter(vec![4, 5, 6]));
//! merge.close();
//!
//! while let Some(item) = merge.next().await {
//!     println!("{}", item);
//! }
//! # }
//! ```
//!
//! ## 2. Handle-based API with `dynamic_merge_with_handle`
//!
//! Use the handle-based API when you need separate mutable references to the stream
//! and the push functionality. This is useful when different parts of your code need
//! to own and mutate each component independently:
//!
//! ```rust
//! use futures_concurrency_dynamic::dynamic_merge_with_handle;
//! use futures_util::stream::{self, StreamExt};
//!
//! # async fn example() {
//! let (mut stream, mut handle) = dynamic_merge_with_handle::<i32>();
//!
//! // One task can push streams
//! let producer = tokio::spawn(async move {
//!     handle.push(stream::iter(vec![1, 2, 3]));
//!     handle.push(stream::iter(vec![4, 5, 6]));
//!     handle.close();
//! });
//!
//! // Another task can consume from the stream
//! let consumer = tokio::spawn(async move {
//!     while let Some(item) = stream.next().await {
//!         println!("{}", item);
//!     }
//! });
//!
//! producer.await.unwrap();
//! consumer.await.unwrap();
//! # }
//! ```

pub mod dynamic_merge;
pub mod handle;

pub use dynamic_merge::DynamicMerge;
pub use handle::{dynamic_merge_with_handle, DynamicMergeHandle, DynamicMergeStream};
