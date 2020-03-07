use crossbeam_utils;
use num_cpus;

use std::num::NonZeroUsize;

use scopeguard::defer;
use snafu::Snafu;

#[must_use]
#[derive(Copy, Clone, Debug)]
pub enum Continue {
    Continue,
    Stop,
}

#[derive(Copy, Clone, Debug)]
pub enum WorkerCount {
    Auto,
    Manual(NonZeroUsize),
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
    worker_count: WorkerCount,
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

    let worker_count = match worker_count {
        WorkerCount::Auto => num_cpus::get(),
        WorkerCount::Manual(num) => num.get(),
    };

    // References that can safely be moved into the thread
    let iterator = &iterator;
    let init_fun = &init_fun;
    let worker_fun = &worker_fun;

    crossbeam_utils::thread::scope(|scope| -> Result<(), ParallelForError<Ei, Ew, Eb>> {
        let handles = (0..worker_count).map(|worker_id| {
            scope.spawn(move |_| -> Result<(), ParallelForError<Ei, Ew, Eb>> {
                defer! {
                    (*iterator.lock().unwrap()).kill(); // Stop all threads if we're running out from the loop (even when panicking)
                }
                let mut state = init_fun(worker_id)
                    .map_err(|source| ParallelForError::InitTaskError{source})?;

                #[allow(clippy::while_let_loop)]
                loop {
                    let item = {
                        let mut iterator_guard = iterator.lock().unwrap();
                        match (*iterator_guard).next() {
                            Some(item) => item,
                            None => {
                                (*iterator_guard).kill();
                                break;
                            },
                        }
                    };
                    worker_fun(&mut state, item)
                        .map_err(|source| ParallelForError::WorkerTaskError{source})?;
                };

                Ok(())
            })
        }).collect::<Vec<_>>();
        let background_result = background_fun()
            .map_err(|source| {
                (*iterator.lock().unwrap()).kill();
                ParallelForError::BackgroundTaskError { source }
            })?;

        match background_result {
            Continue::Continue => {},
            Continue::Stop => (*iterator.lock().unwrap()).kill(),
        };

        for handle in handles {
            handle.join().unwrap()?;
        }


        Ok(())
    })
    .unwrap() // We have already propagated panics
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

#[cfg(test)]
mod test {
    use super::*;
    use panic_control;
    use proptest::prelude::*;

    fn worker_count_strategy() -> impl Strategy<Value = WorkerCount> {
        prop_oneof![
            Just(WorkerCount::Auto),
            (1..32usize).prop_map(|n| WorkerCount::Manual(NonZeroUsize::new(n).unwrap())),
        ]
    }

    proptest! {
        // Checks that each worker has the same thread id as the state
        #[test]
        fn stable_thread_id(worker_count in worker_count_strategy(), n in 0..1000u32) {
            parallel_for_each(
                0..n,
                |_worker_id| -> Result<_, NoError> { Ok(std::thread::current().id()) },
                |state_thread_id, _i| -> Result<(), NoError> {
                    assert_eq!(&std::thread::current().id(), state_thread_id);
                    Ok(())
                },
                || -> Result<_, NoError> { Ok(Continue::Continue) },
                worker_count).unwrap();
        }

        /// Sums a range using pralellel_for_each, checks that sum is as expected
        #[test]
        fn sum(worker_count in worker_count_strategy(), n in 0..1000u32) {
            let sum = std::sync::Mutex::new(0u32);

            parallel_for_each(
                0..n,
                |_worker_id| -> Result<(), NoError> { Ok(()) },
                |_state, i| -> Result<(), NoError> {
                    (*sum.lock().unwrap()) += i;
                    Ok(())
                },
                || -> Result<_, NoError> { Ok(Continue::Continue) },
                worker_count).unwrap();

            assert_eq!((*sum.lock().unwrap()), if n > 0 { n * (n - 1) / 2 } else { 0 });
        }

        /// Sums a range using pralellel_for_each, keeping the partial sums in shared state, checks
        /// that sum is as expected
        #[test]
        fn sum_in_state(worker_count in worker_count_strategy(), n in 0..1000u32) {
            let sum = std::sync::Mutex::new(0u32);

            struct State<'a> {
                local_sum: u32,
                global_sum: &'a std::sync::Mutex<u32>,
            }

            impl<'a> Drop for State<'a> {
                fn drop(&mut self) {
                    (*self.global_sum.lock().unwrap()) += self.local_sum;
                }
            }

            parallel_for_each(
                0..n,
                |_worker_id| -> Result<_, NoError> { Ok(State { local_sum: 0, global_sum: &sum }) },
                |state, i| -> Result<(), NoError> {
                    state.local_sum += i;
                    Ok(())
                },
                || -> Result<_, NoError> { Ok(Continue::Continue) },
                worker_count).unwrap();

            assert_eq!((*sum.lock().unwrap()), if n > 0 { n * (n - 1) / 2 } else { 0 });
        }

        /// Checks that the jobs are actually running in different threads by
        /// blocking as many threads as there are workers.
        #[test]
        fn actual_threads(worker_count in 1..10usize) {
            let count_waiting = std::sync::Mutex::new(0usize);
            let cond = std::sync::Condvar::new();

            let end = std::time::Instant::now() + std::time::Duration::from_secs(2);

            parallel_for_each(
                0..worker_count,
                |_worker_id| -> Result<(), NoError> {
                    let mut count_waiting = count_waiting.lock().unwrap();
                    *count_waiting += 1;
                    if *count_waiting >= worker_count {
                        cond.notify_all();
                    } else {
                        loop {
                            let result = cond.wait_timeout(
                                count_waiting,
                                end - std::time::Instant::now()).unwrap();
                            count_waiting = result.0;
                            if result.1.timed_out() || *count_waiting >= worker_count {
                                break;
                            }
                        }
                    };
                    Ok(())
                },
                |_state, _i| -> Result<(), NoError> { Ok(()) },
                || -> Result<_, NoError> { Ok(Continue::Continue) },
                WorkerCount::Manual(NonZeroUsize::new(worker_count).unwrap())).unwrap();

            assert_eq!(*count_waiting.lock().unwrap(), worker_count);
        }

        /// Checks that the iteration stops when background function returns Stop.
        #[test]
        fn stop_from_background(worker_count in worker_count_strategy()) {
            let end = std::time::Instant::now() + std::time::Duration::from_secs(2);

            parallel_for_each(
                0..,
                |_worker_id| -> Result<(), NoError> {
                    panic_control::disable_hook_in_current_thread(); // Disable panic hookks to keep the output clean in case the test fails
                    Ok(())
                },
                |_state, _i| -> Result<(), NoError> {
                    assert!(std::time::Instant::now() < end);
                    Ok(())
                },
                || -> Result<_, NoError> { Ok(Continue::Stop) },
                worker_count).unwrap();
        }

        /// Checks that panics from thread init function are propagated
        #[test]
        #[should_panic]
        fn propagates_panics_init(worker_count in worker_count_strategy(), n in 0..1000u32) {
            parallel_for_each(
                0..n,
                |_worker_id| -> Result<(), NoError> {
                    panic_control::disable_hook_in_current_thread();
                    panic!("Don't panic!");
                },
                |_state, _i| -> Result<(), NoError> { Ok(()) },
                || -> Result<_, NoError> { Ok(Continue::Continue) },
                worker_count).unwrap();
        }

        /// Checks that panics from thread init function are propagated
        #[test]
        #[should_panic]
        fn propagates_panics_worker(worker_count in worker_count_strategy(), n in 0..1000u32) {
            parallel_for_each(
                0..n,
                |_worker_id| -> Result<(), NoError> {
                    panic_control::disable_hook_in_current_thread();
                    Ok(())
                },
                |_state, _i| -> Result<(), NoError> {
                    panic!("Don't panic!");
                },
                || -> Result<_, NoError> { Ok(Continue::Continue) },
                worker_count).unwrap();
        }

        /// Checks that panics from thread init function are propagated
        #[test]
        #[should_panic]
        fn propagates_panics_background(worker_count in worker_count_strategy(), n in 0..1000u32) {
            parallel_for_each(
                0..n,
                |_worker_id| -> Result<(), NoError> { Ok(()) },
                |_state, _i| -> Result<(), NoError> { Ok(()) },
                || -> Result<_, NoError> { panic!("Don't panic!"); },
                worker_count).unwrap();
        }

        #[test]
        fn ugly_iterator(worker_count in worker_count_strategy(), n in 0..1000u32) {
            struct UglyIterator(u32);

            impl Iterator for UglyIterator {
                type Item = u32;
                fn next(&mut self) -> Option<u32> {
                    if self.0 == 0 {
                        Some(1)
                    } else {
                        self.0 -= 1;
                        if self.0 == 0 {
                            None
                        } else {
                            Some(1)
                        }
                    }
                }
            }

            let sum = std::sync::Mutex::new(0u32);

            parallel_for_each(
                UglyIterator(n + 1),
                |_worker_id| -> Result<(), NoError> { Ok(()) },
                |_state, i| -> Result<(), NoError> {
                    (*sum.lock().unwrap()) += i;
                    Ok(())
                },
                || -> Result<_, NoError> { Ok(Continue::Continue) },
                worker_count).unwrap();

            assert_eq!((*sum.lock().unwrap()), n);
        }

        /// Checks that the iteration stops when background function returns Stop.
        #[test]
        fn error_from_init(worker_count in worker_count_strategy()) {
            let end = std::time::Instant::now() + std::time::Duration::from_secs(2);

            let result = parallel_for_each(
                0..,
                |worker_id| -> Result<(), String> {
                    if worker_id == 0 {
                        Err("None shall pass!".to_string())
                    } else {
                        Ok(())
                    }
                },
                |_state, _i| -> Result<(), NoError> {
                    assert!(std::time::Instant::now() < end);
                    Ok(())
                },
                || -> Result<_, NoError> { Ok(Continue::Continue) },
                worker_count);

            if let Err(ParallelForError::InitTaskError{..}) = result {}
            else {
                panic!("We didn't get the right error");
            }
        }

        /// Checks that the iteration stops when background function returns Stop.
        #[test]
        fn error_from_worker(worker_count in worker_count_strategy(), n in 0..100u32) {
            let end = std::time::Instant::now() + std::time::Duration::from_secs(2);

            let result = parallel_for_each(
                0..,
                |_worker_id| -> Result<(), NoError> {
                    Ok(())
                },
                |_state, i| -> Result<(), String> {
                    assert!(std::time::Instant::now() < end);
                    if i == n {
                        Err("None shall pass!".to_string())
                    } else {
                        Ok(())
                    }
                },
                || -> Result<_, NoError> { Ok(Continue::Continue) },
                worker_count);

            if let Err(ParallelForError::WorkerTaskError{..}) = result {}
            else {
                panic!("We didn't get the right error");
            }
        }

        /// Checks that the iteration stops when background function returns Stop.
        #[test]
        fn error_from_background(worker_count in worker_count_strategy()) {
            let end = std::time::Instant::now() + std::time::Duration::from_secs(2);

            let result = parallel_for_each(
                0..,
                |_worker_id| -> Result<(), NoError> {
                    Ok(())
                },
                |_state, _i| -> Result<(), NoError> {
                    assert!(std::time::Instant::now() < end);
                    Ok(())
                },
                || -> Result<_, String> { Err("None shall pass!".to_string()) },
                worker_count);

            if let Err(ParallelForError::BackgroundTaskError{..}) = result {}
            else {
                panic!("We didn't get the right error");
            }
        }
    }
}
