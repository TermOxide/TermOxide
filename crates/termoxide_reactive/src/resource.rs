//! # Resource — reactive asynchronous loading
//!
//! [`Resource<T>`] integrates asynchronous data loading.
//! The result is then exposed via an internal [`Signal`](crate::signal::Signal).

use crate::signal::Signal;
use std::fmt;
use std::future::Future;

/// Loading state of a [`Resource`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceState<T> {
    Loading,
    Ready(T),
    Error(String),
}

impl<T> ResourceState<T> {
    pub fn is_loading(&self) -> bool {
        matches!(self, ResourceState::Loading)
    }

    pub fn is_ready(&self) -> bool {
        matches!(self, ResourceState::Ready(_))
    }

    pub fn value(&self) -> Option<&T> {
        if let ResourceState::Ready(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

/// Asynchronous data loading.
///
/// Reactive contexts that read [`Resource::state`] or [`Resource::get`]
/// are automatically notified when the loading completes.
///
/// # Example
///
/// ```no_run
/// use termoxide_reactive::{Resource, Effect, runtime::with_owner};
///
/// with_owner(|| {
///     let user = Resource::new(|| async {
///         // perform the asynchronous work here
///         String::from("Alice")
///     });
///
///     assert!(user.state_untracked().is_loading());
///
///     Effect::new(move |_prev| {
///         if let Some(name) = user.get() {
///             println!("User loaded: {}", name);
///             assert!(!user.state_untracked().is_loading());
///         }
///     });
/// });
/// ```
pub struct Resource<T: Clone + Send + Sync + 'static> {
    state: Signal<ResourceState<T>>,
}

impl<T: Clone + Send + Sync + 'static> Copy for Resource<T> {}
impl<T: Clone + Send + Sync + 'static> Clone for Resource<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T: Clone + Send + Sync + 'static> Resource<T> {
    /// Create a new asynchronous resource.
    ///
    /// `fetcher` is a closure invoked immediately.
    /// The resulting data is available via [`state`](Resource::state)
    /// or [`get`](Resource::get) once the `Future` resolves.
    ///
    /// For a version that unwrap the error, see ['Resource::new_fallible'].
    pub fn new<F, Fut>(fetcher: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + Send + 'static,
    {
        let state: Signal<ResourceState<T>> = Signal::new(ResourceState::Loading);

        any_spawner::Executor::spawn(async move {
            let result = fetcher().await;
            state.set(ResourceState::Ready(result));
        });

        Self { state }
    }

    /// Create an asynchronous resource whose fetcher may fail.
    ///
    /// On error, the state becomes [`ResourceState::Error`].
    pub fn new_fallible<F, Fut, E>(fetcher: F) -> Self
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        E: fmt::Display + Send + 'static,
    {
        let state: Signal<ResourceState<T>> = Signal::new(ResourceState::Loading);

        any_spawner::Executor::spawn(async move {
            match fetcher().await {
                Ok(val) => state.set(ResourceState::Ready(val)),
                Err(e) => state.set(ResourceState::Error(e.to_string())),
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

    /// Return the error message if available, or `node`otherwise.
    ///
    /// Registers a reactive dependency.
    pub fn error(&self) -> Option<String> {
        match self.state() {
            ResourceState::Error(msg) => Some(msg.clone()),
            _ => None,
        }
    }

    pub fn as_signal(&self) -> &Signal<ResourceState<T>> {
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
