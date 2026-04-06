use std::fmt::Display;
use std::panic::Location;

fn with_callsite<C: Display>(context: C, location: &'static Location<'static>) -> String {
    format!("{context} (at {}:{})", location.file(), location.line())
}

/// Drop-in replacement for `anyhow::Context` that appends call-site location.
///
/// Usage in a module:
/// `use crate::error_context::Context;`
pub trait Context<T> {
    #[track_caller]
    fn context<C>(self, context: C) -> anyhow::Result<T>
    where
        C: Display + Send + Sync + 'static;

    #[track_caller]
    fn with_context<C, F>(self, f: F) -> anyhow::Result<T>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C;
}

impl<T, E> Context<T> for Result<T, E>
where
    E: Into<anyhow::Error>,
{
    #[track_caller]
    fn context<C>(self, context: C) -> anyhow::Result<T>
    where
        C: Display + Send + Sync + 'static,
    {
        let caller = Location::caller();
        self.map_err(Into::into)
            .map_err(|e| e.context(with_callsite(context, caller)))
    }

    #[track_caller]
    fn with_context<C, F>(self, f: F) -> anyhow::Result<T>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        let caller = Location::caller();
        self.map_err(Into::into)
            .map_err(|e| e.context(with_callsite(f(), caller)))
    }
}

impl<T> Context<T> for Option<T> {
    #[track_caller]
    fn context<C>(self, context: C) -> anyhow::Result<T>
    where
        C: Display + Send + Sync + 'static,
    {
        let caller = Location::caller();
        anyhow::Context::context(self, with_callsite(context, caller))
    }

    #[track_caller]
    fn with_context<C, F>(self, f: F) -> anyhow::Result<T>
    where
        C: Display + Send + Sync + 'static,
        F: FnOnce() -> C,
    {
        let caller = Location::caller();
        anyhow::Context::context(self, with_callsite(f(), caller))
    }
}
