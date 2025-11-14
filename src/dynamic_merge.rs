//! Dynamic merge combinator using SelectAll from futures-util.
//!
//! This module provides a type alias to `SelectAll` that handles the boxing internally
//! for a more ergonomic API.

use futures_core::Stream;
use futures_util::stream::SelectAll;
use std::pin::Pin;

/// A dynamically mergeable collection of streams built on top of SelectAll.
///
/// This is a type alias for `futures_util::stream::SelectAll` with boxed streams
/// to provide a more ergonomic API for dynamic stream merging.
///
/// # Examples
///
/// ```
/// use futures_concurrency_dynamic::DynamicMerge;
/// use futures_util::stream::{self, StreamExt};
///
/// # async fn example() {
/// let mut merge = DynamicMerge::new();
/// merge.push(Box::pin(stream::iter(vec![1, 2, 3])));
/// merge.push(Box::pin(stream::iter(vec![4, 5, 6])));
///
/// while let Some(item) = merge.next().await {
///     println!("{}", item);
/// }
/// # }
/// ```
pub type DynamicMerge<'a, T> = SelectAll<Pin<Box<dyn Stream<Item = T> + Send + 'a>>>;
