use crossbeam_utils;
use num_cpus;
use parking_lot;
use scopeguard;
use snafu::Snafu;

use std::num::NonZeroUsize;

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
pub enum ParallelForEachError<Ei, Ew, Eb>
where
    Ei: ErrorSource,
    Ew: ErrorSource,
    Eb: ErrorSource,
{
    InitTaskError { source: Ei },
    WorkerTaskError { source: Ew },
    BackgroundTaskError { source: Eb },
}

impl<Ei, Ew, Eb> std::fmt::Display for ParallelForEachError<Ei, Ew, Eb>
where
    Ei: ErrorSource,
    Ew: ErrorSource,
    Eb: ErrorSource,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::InitTaskError { .. } => write!(f, "Init task failed"),
            Self::WorkerTaskError { .. } => write!(f, "Worker task failed"),
            Self::BackgroundTaskError { .. } => write!(f, "Background task failed"),
        }
    }
}

impl<Ei, Ew, Eb> std::error::Error for ParallelForEachError<Ei, Ew, Eb>
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
pub fn parallel_for_each<It, Fi, Fw, Fb, Ff, Ei, Ew, Eb, State>(
    iterator: It,
    init_fun: Fi,
    worker_fun: Fw,
    background_fun: Fb,
    finished_callback: Ff,
    worker_count: WorkerCount,
) -> Result<(), ParallelForEachError<Ei, Ew, Eb>>
where
    It: Iterator + Send,
    Fi: Fn(usize) -> Result<State, Ei> + Sync + Send,
    Fw: Fn(&mut State, It::Item) -> Result<(), Ew> + Sync + Send,
    Fb: FnOnce() -> Result<Continue, Eb>,
    Ff: Fn() -> () + Sync + Send,
    Ei: ErrorSource,
    Ew: ErrorSource,
    Eb: ErrorSource,
{
    struct State<T> {
        iterator: Option<T>,
        threads_running: usize,
    }

    impl<T: Iterator> State<T> {
        /// Behaves like iterator next
        fn next(&mut self) -> Option<<T as Iterator>::Item> {
            let iterator = self.iterator.as_mut()?;
            let item = iterator.next();

            match item {
                Some(_) => {}
                None => self.stop(),
            };

            item
        }

        fn stop(&mut self) {
            self.iterator = None
        }
    }

    let worker_count = match worker_count {
        WorkerCount::Auto => num_cpus::get(),
        WorkerCount::Manual(num) => num.get(),
    };

    let state = parking_lot::Mutex::new(State {
        iterator: Some(iterator),
        threads_running: worker_count,
    });

    // References that can safely be moved into the thread
    let state = &state;
    let init_fun = &init_fun;
    let worker_fun = &worker_fun;
    let finished_callback = &finished_callback;

    crossbeam_utils::thread::scope(|scope| -> Result<(), ParallelForEachError<Ei, Ew, Eb>> {
        let handles = (0..worker_count).map(|worker_id| {
            scope.spawn(move |_| -> Result<(), ParallelForEachError<Ei, Ew, Eb>> {
                let mut state = scopeguard::guard(state.lock(), |mut state| {
                    state.stop(); // Stop all threads if we're running out from the loop (even when panicking)
                    state.threads_running -= 1;
                    if state.threads_running == 0 {
                        parking_lot::lock_api::MutexGuard::unlocked(&mut state, || finished_callback());
                    }
                });
                let mut thread_state = parking_lot::lock_api::MutexGuard::unlocked(&mut state, || init_fun(worker_id))
                    .map_err(|source| ParallelForEachError::InitTaskError{source})?;

                #[allow(clippy::while_let_loop)]
                loop {
                    let item = match (*state).next() {
                        Some(item) => item,
                        None => break,
                    };
                    parking_lot::lock_api::MutexGuard::unlocked(&mut state, || worker_fun(&mut thread_state, item))
                        .map_err(|source| ParallelForEachError::WorkerTaskError{source})?
                };

                Ok(())
            })
        }).collect::<Vec<_>>();

        scopeguard::defer_on_unwind! {
            state.lock().stop()
        }

        let background_result = background_fun()
            .map_err(|source| ParallelForEachError::BackgroundTaskError{source});

        match background_result {
            Ok(Continue::Continue) => {},
            _ => (*state.lock()).stop(),
        };

        let _ = background_result?;

        for handle in handles {
            handle.join().unwrap()?;
        }

        Ok(())
    })
    .unwrap() // We have already propagated panics
    ?;

    Ok(())
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
            (1..32usize).prop_map(|n| WorkerCount::Manual(NonZeroUsize::new(n).unwrap())),
            Just(WorkerCount::Auto),
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
                || {},
                worker_count).unwrap();
        }

        /// Sums a range using pralellel_for_each, checks that sum is as expected
        #[test]
        fn sum(worker_count in worker_count_strategy(), n in 0..1000u32) {
            let sum = std::sync::atomic::AtomicU32::new(0);

            parallel_for_each(
                0..n,
                |_worker_id| -> Result<(), NoError> { Ok(()) },
                |_state, i| -> Result<(), NoError> {
                    sum.fetch_add(i, std::sync::atomic::Ordering::Relaxed);
                    Ok(())
                },
                || -> Result<_, NoError> { Ok(Continue::Continue) },
                || {},
                worker_count).unwrap();

            assert_eq!(sum.load(std::sync::atomic::Ordering::Relaxed), if n > 0 { n * (n - 1) / 2 } else { 0 });
        }

        /// Sums a range using pralellel_for_each, keeping the partial sums in shared state, checks
        /// that sum is as expected
        #[test]
        fn sum_in_state(worker_count in worker_count_strategy(), n in 0..1000u32) {
            let sum = std::sync::atomic::AtomicU32::new(0);

            struct State<'a> {
                local_sum: u32,
                global_sum: &'a std::sync::atomic::AtomicU32,
            }

            impl<'a> Drop for State<'a> {
                fn drop(&mut self) {
                    self.global_sum.fetch_add(self.local_sum, std::sync::atomic::Ordering::Relaxed);
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
                || {},
                worker_count).unwrap();

            assert_eq!(sum.load(std::sync::atomic::Ordering::Relaxed), if n > 0 { n * (n - 1) / 2 } else { 0 });
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
                |_worker_id| -> Result<(), String> {
                    let mut count_waiting = count_waiting.lock().unwrap();
                    *count_waiting += 1;
                    if *count_waiting >= worker_count {
                        cond.notify_all();
                        Ok(())
                    } else {
                        while let Some(timeout) = end.checked_duration_since(std::time::Instant::now()) {
                            let result = cond.wait_timeout(
                                count_waiting,
                                timeout).unwrap();
                            count_waiting = result.0;
                            if result.1.timed_out() {
                                return Err("Timed out".into());
                            }
                            else if *count_waiting >= worker_count {
                                return Ok(());
                            }
                        }
                        Err("wtf?".into())
                    }
                },
                |_state, _i| -> Result<(), NoError> { Ok(()) },
                || -> Result<_, NoError> { Ok(Continue::Continue) },
                || {},
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
                || {},
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
                || {},
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
                || {},
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
                || {},
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
                || {},
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
                || {},
                worker_count);

            if let Err(ParallelForEachError::InitTaskError{..}) = result {}
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
                || {},
                worker_count);

            if let Err(ParallelForEachError::WorkerTaskError{..}) = result {}
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
                || {},
                worker_count);

            if let Err(ParallelForEachError::BackgroundTaskError{..}) = result {}
            else {
                panic!("We didn't get the right error");
            }
        }
    }
}
