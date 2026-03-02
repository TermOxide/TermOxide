//! # Resource — reactive asynchronous loading
//!
//! [`Resource<T>`] integrates asynchronous data loading into the reactive
//! graph. The request is launched on the Tokio runtime; the result is then
//! exposed via an internal [`ArcRwSignal`](crate::signal::ArcRwSignal) so that
//! any subscribed reactive context is notified once the data becomes available.

use crate::signal::ArcRwSignal;
use std::fmt;
use std::future::Future;

/// Loading state of a [`Resource`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceState<T> {
    /// Loading is in progress.
    Loading,
    /// The data is available.
    Ready(T),
    /// Loading failed.
    Error(String),
}

impl<T> ResourceState<T> {
    /// Returns `true` if loading is in progress.
    pub fn is_loading(&self) -> bool {
        matches!(self, ResourceState::Loading)
    }

    /// Returns `true` if the data is available.
    pub fn is_ready(&self) -> bool {
        matches!(self, ResourceState::Ready(_))
    }

    /// Returns a reference to the value if available.
    pub fn value(&self) -> Option<&T> {
        if let ResourceState::Ready(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

/// Asynchronous data loading integrated into the reactive graph.
///
/// The `fetcher` function is executed once on the Tokio runtime.
/// Reactive contexts that read [`Resource::state`] or [`Resource::get`]
/// are automatically notified when the loading completes.
///
/// # Example
///
/// ```no_run
/// use termoxide_reactive::{Resource, runtime::with_owner};
///
/// #[tokio::main]
/// async fn main() {
///     let rt = tokio::runtime::Handle::current();
///     with_owner(|| {
///         let user = Resource::new(|| async {
///             // simulate a network call
///             tokio::time::sleep(std::time::Duration::from_millis(10)).await;
///             String::from("Alice")
///         });
///
///         // Immediately: Loading state
///         assert!(user.is_loading());
///     });
/// }
/// ```
#[derive(Clone)]
pub struct Resource<T: Clone + Send + Sync + 'static> {
    state: ArcRwSignal<ResourceState<T>>,
}

impl<T: Clone + Send + Sync + 'static> Resource<T> {
    /// Create a new asynchronous resource.
    ///
    /// `fetcher` is a closure `Fn() -> impl Future<Output = T>` invoked
    /// immediately. The resulting data is available via [`state`](Resource::state)
    /// or [`get`](Resource::get) once the `Future` resolves.
    pub fn new<F, Fut>(fetcher: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
    {
        let state: ArcRwSignal<ResourceState<T>> = ArcRwSignal::new(ResourceState::Loading);

        let state_clone = state.clone();

        tokio::spawn(async move {
            let result = fetcher().await;
            state_clone.set(ResourceState::Ready(result));
        });

        Self { state }
    }

    /// Create an asynchronous resource whose fetcher may fail.
    ///
    /// On error, the state becomes [`ResourceState::Error`],
    /// avoiding panicking the reactive graph.
    pub fn new_fallible<F, Fut, E>(fetcher: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        E: fmt::Display + Send + 'static,
    {
        let state: ArcRwSignal<ResourceState<T>> = ArcRwSignal::new(ResourceState::Loading);

        let state_clone = state.clone();

        tokio::spawn(async move {
            match fetcher().await {
                Ok(val) => state_clone.set(ResourceState::Ready(val)),
                Err(e) => state_clone.set(ResourceState::Error(e.to_string())),
            }
        });

        Self { state }
    }

    /// Returns the current state **registering a reactive dependency**.
    pub fn state(&self) -> ResourceState<T> {
        self.state.get()
    }

    /// Returns the current state **without creating a dependency**.
    pub fn state_untracked(&self) -> ResourceState<T> {
        self.state.get_untracked()
    }

    /// Returns `true` if loading is still in progress.
    ///
    /// Registers a reactive dependency.
    pub fn is_loading(&self) -> bool {
        self.state().is_loading()
    }

    /// Returns the value if available, or `None` otherwise.
    ///
    /// Registers a reactive dependency.
    pub fn get(&self) -> Option<T> {
        match self.state() {
            ResourceState::Ready(v) => Some(v),
            _ => None,
        }
    }

    /// Returns the inner signal for advanced subscriptions.
    pub fn as_signal(&self) -> &ArcRwSignal<ResourceState<T>> {
        &self.state
    }
}

impl<T: fmt::Debug + Clone + Send + Sync + 'static> fmt::Debug for Resource<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Resource")
            .field("state", &self.state.get_untracked())
            .finish()
    }
}
