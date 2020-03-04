use crossbeam_utils;
use num_cpus;

use scopeguard::defer;
use snafu::Snafu;

#[must_use]
pub enum Continue {
    Continue,
    Stop,
}

#[derive(Debug)]
pub enum ParallelForError<Ei, Ew, Eb>
where
    Ei: ErrorSource,
    Ew: ErrorSource,
    Eb: ErrorSource,
{
    InitTaskError { source: Ei },
    WorkerTaskError { source: Ew },
    BackgroundTaskError { source: Eb },
}

impl<Ei, Ew, Eb> std::fmt::Display for ParallelForError<Ei, Ew, Eb>
where
    Ei: ErrorSource,
    Ew: ErrorSource,
    Eb: ErrorSource,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::InitTaskError { source: _ } => write!(f, "Init task failed"),
            Self::WorkerTaskError { source: _ } => write!(f, "Worker task failed"),
            Self::BackgroundTaskError { source: _ } => write!(f, "Background task failed"),
        }
    }
}

impl<Ei, Ew, Eb> std::error::Error for ParallelForError<Ei, Ew, Eb>
where
    Ei: ErrorSource,
    Ew: ErrorSource,
    Eb: ErrorSource,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InitTaskError { source } => source.source(),
            Self::WorkerTaskError { source } => source.source(),
            Self::BackgroundTaskError { source } => source.source(),
        }
    }
}

#[derive(Debug, Snafu)]
pub enum NoError {}

/// Runs a worker function for each item of an iterator in multiple threads.
/// Allows a per-thread init function and a background function that runs in the main thread
/// while the workers are processing.
pub fn parallel_for_each<It, Fi, Fw, Fb, Ei, Ew, Eb, State>(
    iterator: It,
    init_fun: Fi,
    worker_fun: Fw,
    background_fun: Fb,
) -> Result<(), ParallelForError<Ei, Ew, Eb>>
where
    It: Iterator + Send,
    Fi: Fn(usize) -> Result<State, Ei> + Sync + Send,
    Fw: Fn(&mut State, It::Item) -> Result<(), Ew> + Sync + Send,
    Fb: FnOnce() -> Result<Continue, Eb>,
    Ei: ErrorSource,
    Ew: ErrorSource,
    Eb: ErrorSource,
{
    let iterator = std::sync::Mutex::new(FusedIterator::new(iterator));

    let num_threads = num_cpus::get();

    // References that can safely be moved into the thread
    let iterator = &iterator;
    let init_fun = &init_fun;
    let worker_fun = &worker_fun;

    crossbeam_utils::thread::scope(|scope| -> Result<(), ParallelForError<Ei, Ew, Eb>> {
        for worker_id in 0..num_threads {
            scope.spawn(move |_| -> Result<(), ParallelForError<Ei, Ew, Eb>> {
                defer! {
                    (*iterator.lock().unwrap()).kill(); // Stop all threads if we're running out from the loop
                }
                let mut state = init_fun(worker_id)
                    .map_err(|source| ParallelForError::InitTaskError{source})?;

                #[allow(clippy::while_let_loop)]
                loop {
                    let item = match (*iterator.lock().unwrap()).next() {
                        Some(item) => item,
                        None => {
                            (*iterator.lock().unwrap()).kill();
                            break;
                        },
                    };
                    worker_fun(&mut state, item)
                        .map_err(|source| ParallelForError::WorkerTaskError{source})?;
                };

                Ok(())
            });
        }
        let background_result = background_fun()
            .map_err(|source| {
                (*iterator.lock().unwrap()).kill();
                ParallelForError::BackgroundTaskError { source }
            })?;

        match background_result {
            Continue::Continue => {},
            Continue::Stop => (*iterator.lock().unwrap()).kill(),
        };

        Ok(())
    })
    .unwrap() // Propagate panics from workers
    ?;

    Ok(())
}

/// Iterator that always returns None after the first None and can be
/// artificially stopped from the outside.
struct FusedIterator<T>(Option<T>);

impl<T: Iterator> Iterator for FusedIterator<T> {
    type Item = T::Item;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.as_mut().and_then(|it| it.next()).or_else(|| {
            self.0 = None;
            None
        })
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match &self.0 {
            Some(it) => it.size_hint(),
            None => (0, Some(0)),
        }
    }
}

impl<T> FusedIterator<T> {
    fn new(it: T) -> Self {
        FusedIterator(Some(it))
    }

    /// Make the iterator stop and never return any other value
    fn kill(&mut self) {
        self.0 = None;
    }
}

/// Trait for values that can be used as source error.
pub trait ErrorSource: Sync + Send + std::fmt::Debug {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)>;
}

impl<T> ErrorSource for T
where
    T: std::error::Error + std::fmt::Debug + Send + Sync + 'static,
{
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self)
    }
}

impl ErrorSource for dyn std::error::Error + Send + Sync + 'static {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self)
    }
}

impl<T> ErrorSource for T
where
    T: std::fmt::Debug + Sync + Send,
{
    default fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        None
    }
}
