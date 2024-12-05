use crossbeam_utils;
use num_cpus;
use parking_lot;
use scopeguard;

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

/// Runs a worker function for each item of an iterator in multiple threads.
/// Allows a per-thread initialization function and a background function that runs in the main thread
/// while the workers are processing.
pub fn parallel_for_each<It, Fi, Fw, Fb, Ff, State>(
    iterator: It,
    init_fun: Fi,
    worker_fun: Fw,
    background_fun: Fb,
    finished_callback: Ff,
    worker_count: WorkerCount,
) -> anyhow::Result<()>
where
    It: Iterator + Send,
    Fi: Fn(usize) -> anyhow::Result<State> + Sync,
    Fw: Fn(&mut State, It::Item) -> anyhow::Result<()> + Sync,
    Fb: FnOnce() -> anyhow::Result<Continue>,
    Ff: Fn() -> () + Sync + Send,
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

            if item.is_none() {
                self.stop();
            }

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

    crossbeam_utils::thread::scope(|scope| -> anyhow::Result<()> {
        let handles = (0..worker_count).map(|worker_id| {
            scope.spawn(move |_| -> anyhow::Result<()> {
                let mut state = scopeguard::guard(state.lock(), |mut state| {
                    state.stop(); // Stop all threads if we're running out from the loop (even when panicking)
                    state.threads_running -= 1;
                    if state.threads_running == 0 {
                        parking_lot::lock_api::MutexGuard::unlocked(&mut state, || finished_callback());
                    }
                });
                let mut thread_state = parking_lot::lock_api::MutexGuard::unlocked(&mut state, || init_fun(worker_id))?;

                #[allow(clippy::while_let_loop)]
                loop {
                    let item = match (*state).next() {
                        Some(item) => item,
                        None => break,
                    };
                    parking_lot::lock_api::MutexGuard::unlocked(&mut state, || worker_fun(&mut thread_state, item))?
                };

                Ok(())
            })
        }).collect::<Vec<_>>();

        scopeguard::defer_on_unwind! {
            state.lock().stop()
        }

        let background_result = background_fun();

        match background_result {
            Ok(Continue::Continue) => {},
            _ => (*state.lock()).stop(),
        };

        let _ = background_result?;

        for handle in handles {
            match handle.join() {
                Ok(Ok(())) => {},
                Ok(Err(e)) => return Err(e),
                Err(p) => std::panic::resume_unwind(p),
            }
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

#[cfg(test)]
mod test {
    use super::*;
    use anyhow::bail;
    use assert2::assert;
    use panic_control;
    use proptest::prelude::*;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::time::{Duration, Instant};
    use test_strategy::proptest;

    const TIMEOUT: Duration = Duration::from_secs(2);

    struct IterationCheckHelper {
        finished: AtomicBool,
        latest_end_time: Instant,
    }

    impl IterationCheckHelper {
        fn new() -> IterationCheckHelper {
            IterationCheckHelper {
                finished: AtomicBool::new(false),
                latest_end_time: Instant::now() + TIMEOUT,
            }
        }

        fn workers_running_check(&self) -> anyhow::Result<()> {
            if self.finished.load(Ordering::Relaxed) {
                bail!("Thread is running even though the end callback was encountered")
            } else if Instant::now() > self.latest_end_time {
                bail!("Time limit exceeded")
            } else {
                Ok(())
            }
        }

        fn finished_callback(&self) {
            self.finished.store(true, Ordering::Relaxed);
        }

        fn callback_called_check(&self) -> bool {
            self.finished.load(Ordering::Relaxed)
        }
    }

    impl Arbitrary for WorkerCount {
        type Parameters = ();
        type Strategy = proptest::strategy::BoxedStrategy<Self>;
        fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
            prop_oneof![
                (1..128usize).prop_map(|n| WorkerCount::Manual(NonZeroUsize::new(n).unwrap())),
                Just(WorkerCount::Auto),
            ]
            .boxed()
        }
    }

    // Checks that each worker has the same thread id as the state
    #[proptest]
    fn stable_thread_id(worker_count: WorkerCount, n: u8) {
        let n = n as u32;
        parallel_for_each(
            0..n,
            |_worker_id| Ok(std::thread::current().id()),
            |state_thread_id, _i| {
                assert!(&std::thread::current().id() == state_thread_id);
                Ok(())
            },
            || Ok(Continue::Continue),
            || {},
            worker_count,
        )
        .unwrap();
    }

    /// Sums a range using pralellel_for_each, checks that sum is as expected
    #[proptest]
    fn sum(worker_count: WorkerCount, n: u8) {
        let n = n as u32;
        let helper = IterationCheckHelper::new();
        let sum = std::sync::atomic::AtomicU32::new(0);

        parallel_for_each(
            0..n,
            |_worker_id| helper.workers_running_check(),
            |_state, i| {
                helper.workers_running_check()?;
                sum.fetch_add(i, Ordering::Relaxed);
                Ok(())
            },
            || Ok(Continue::Continue),
            || helper.finished_callback(),
            worker_count,
        )
        .unwrap();

        assert!(helper.callback_called_check());
        assert!(sum.load(Ordering::Relaxed) == if n > 0 { n * (n - 1) / 2 } else { 0 });
    }

    /// Sums a range using pralellel_for_each, keeping the partial sums in shared state, checks
    /// that sum is as expected
    #[proptest]
    fn sum_in_state(worker_count: WorkerCount, n: u8) {
        let n = n as u32;
        let sum = AtomicU32::new(0);

        struct State<'a> {
            local_sum: u32,
            global_sum: &'a AtomicU32,
        }

        impl<'a> Drop for State<'a> {
            fn drop(&mut self) {
                self.global_sum.fetch_add(self.local_sum, Ordering::Relaxed);
            }
        }

        parallel_for_each(
            0..n,
            |_worker_id| {
                Ok(State {
                    local_sum: 0,
                    global_sum: &sum,
                })
            },
            |state, i| {
                state.local_sum += i;
                Ok(())
            },
            || Ok(Continue::Continue),
            || {},
            worker_count,
        )
        .unwrap();

        assert!(sum.load(Ordering::Relaxed) == if n > 0 { n * (n - 1) / 2 } else { 0 });
    }

    /// Checks that the jobs are actually running in different threads by
    /// blocking as many threads as there are workers.
    #[proptest]
    fn actual_threads(worker_count: WorkerCount) {
        let n = match worker_count {
            WorkerCount::Auto => {
                prop_assume!(
                    false,
                    "This test doesn't work with automatic number of threads"
                );
                unreachable!();
            }
            WorkerCount::Manual(n) => n.get(),
        };

        let count_waiting = std::sync::Mutex::new(0usize);
        let cond = std::sync::Condvar::new();

        let end = Instant::now() + TIMEOUT;

        parallel_for_each(
            0..n,
            |_worker_id| {
                let mut count_waiting = count_waiting.lock().unwrap();
                *count_waiting += 1;
                if *count_waiting >= n {
                    cond.notify_all();
                    Ok(())
                } else {
                    while let Some(timeout) = end.checked_duration_since(Instant::now()) {
                        let result = cond.wait_timeout(count_waiting, timeout).unwrap();
                        count_waiting = result.0;
                        if result.1.timed_out() {
                            bail!("Timed out");
                        } else if *count_waiting >= n {
                            return Ok(());
                        }
                    }
                    bail!("wtf?")
                }
            },
            |_state, _i| Ok(()),
            || Ok(Continue::Continue),
            || {},
            worker_count,
        )
        .unwrap();

        assert!(*count_waiting.lock().unwrap() == n);
    }

    /// Checks that the iteration stops when background function returns Stop and that finished
    /// callback is correctly invoked.
    #[proptest]
    fn stop_from_background(worker_count: WorkerCount) {
        let helper = IterationCheckHelper::new();

        parallel_for_each(
            0..,
            |_worker_id| helper.workers_running_check(),
            |_state, _i| helper.workers_running_check(),
            || {
                // Here we can check that the threads have not finished yet, because the
                // iterator is infinite and only waiting for this method to return
                helper.workers_running_check()?;
                Ok(Continue::Stop)
            },
            || {
                helper.finished_callback();
            },
            worker_count,
        )
        .unwrap();
        assert!(helper.callback_called_check());
    }

    /// Checks that panics from thread init function are propagated
    #[proptest]
    fn propagates_panics_init(worker_count: WorkerCount) {
        let helper = IterationCheckHelper::new();
        let result = std::panic::catch_unwind(|| {
            parallel_for_each(
                0..,
                |worker_id| {
                    helper.workers_running_check()?;
                    if worker_id == 0 {
                        panic_control::disable_hook_in_current_thread();
                        panic!("Don't panic!");
                    } else {
                        Ok(())
                    }
                },
                |_state, _i| helper.workers_running_check(),
                || Ok(Continue::Continue),
                || helper.finished_callback(),
                worker_count,
            )
        });
        match result {
            Err(e) => {
                if let Some(string) = e.downcast_ref::<&str>() {
                    assert!(string == &"Don't panic!");
                    assert!(helper.callback_called_check());
                } else {
                    panic!("Got non-string panic");
                }
            }
            Ok(Ok(_)) => panic!("Didn't get panic"),
            Ok(Err(e)) => panic!("Something went wrong: {}", e),
        }
    }

    /// Checks that panics from thread init function are propagated
    #[proptest]
    fn propagates_panics_worker(worker_count: WorkerCount, n: u8) {
        let n = n as u32;
        let helper = IterationCheckHelper::new();
        let result = std::panic::catch_unwind(|| {
            parallel_for_each(
                0..,
                |_worker_id| {
                    panic_control::disable_hook_in_current_thread();
                    helper.workers_running_check()
                },
                |_state, i| {
                    helper.workers_running_check()?;
                    if i == n {
                        panic!("Don't panic!");
                    } else {
                        Ok(())
                    }
                },
                || Ok(Continue::Continue),
                || helper.finished_callback(),
                worker_count,
            )
        });
        match result {
            Err(e) => {
                if let Some(string) = e.downcast_ref::<&str>() {
                    assert!(string == &"Don't panic!");
                    assert!(helper.callback_called_check());
                } else {
                    panic!("Got non-string panic");
                }
            }
            Ok(Ok(_)) => panic!("Didn't get panic"),
            Ok(Err(e)) => panic!("Something went wrong: {}", e),
        }
    }

    /// Checks that panics from thread init function are propagated
    #[proptest]
    fn propagates_panics_background(worker_count: WorkerCount) {
        let helper = IterationCheckHelper::new();
        let result = std::panic::catch_unwind(|| {
            parallel_for_each(
                0..,
                |_worker_id| helper.workers_running_check(),
                |_state, _i| helper.workers_running_check(),
                || {
                    helper.workers_running_check()?;
                    panic_control::disable_hook_in_current_thread();
                    panic!("Don't panic!");
                },
                || helper.finished_callback(),
                worker_count,
            )
        });
        match result {
            Err(e) => {
                if let Some(string) = e.downcast_ref::<&str>() {
                    assert!(string == &"Don't panic!");
                    assert!(helper.callback_called_check());
                } else {
                    panic!("Got non-string panic");
                }
            }
            Ok(Ok(_)) => panic!("Didn't get panic"),
            Ok(Err(e)) => panic!("Something went wrong: {}", e),
        }
    }

    /// Checks that panics from finished callback function are propagated
    #[proptest]
    fn propagates_panics_callback(worker_count: WorkerCount) {
        let helper = IterationCheckHelper::new();
        let result = std::panic::catch_unwind(|| {
            parallel_for_each(
                0..,
                |_worker_id| {
                    panic_control::disable_hook_in_current_thread();
                    helper.workers_running_check()
                },
                |_state, _i| helper.workers_running_check(),
                || {
                    helper.workers_running_check()?;
                    Ok(Continue::Stop)
                },
                || {
                    helper.finished_callback();
                    panic!("Don't panic!");
                },
                worker_count,
            )
        });
        match result {
            Err(e) => {
                if let Some(string) = e.downcast_ref::<&str>() {
                    assert!(string == &"Don't panic!");
                    assert!(helper.callback_called_check());
                } else {
                    panic!("Got non-string panic");
                }
            }
            Ok(Ok(_)) => panic!("Didn't get panic"),
            Ok(Err(e)) => panic!("Something went wrong: {}", e),
        }
    }

    /// Tests that if iterator returns None once, it will stop the iteration completely
    #[proptest]
    fn ugly_iterator(worker_count: WorkerCount, n: u8) {
        let n = n as u32;
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

        let sum = AtomicU32::new(0);

        parallel_for_each(
            UglyIterator(n + 1),
            |_worker_id| Ok(()),
            |_state, i| {
                sum.fetch_add(i, Ordering::Relaxed);
                Ok(())
            },
            || Ok(Continue::Continue),
            || {},
            worker_count,
        )
        .unwrap();

        assert!(sum.load(Ordering::Relaxed) == n);
    }

    /// Checks that the iteration stops when background function returns Stop.
    #[proptest]
    fn error_from_init(worker_count: WorkerCount) {
        let helper = IterationCheckHelper::new();

        let result = parallel_for_each(
            0..,
            |worker_id| {
                helper.workers_running_check()?;
                if worker_id == 0 {
                    bail!("None shall pass!")
                } else {
                    Ok(())
                }
            },
            |_state, _i| helper.workers_running_check(),
            || Ok(Continue::Continue),
            || helper.finished_callback(),
            worker_count,
        );

        let msg = format!("{}", result.err().unwrap());
        assert!(msg == "None shall pass!");
    }

    /// Checks that the iteration stops when background function returns Stop.
    #[proptest]
    fn error_from_worker(worker_count: WorkerCount, n: u8) {
        let n = n as u32;
        let helper = IterationCheckHelper::new();

        let result = parallel_for_each(
            0..,
            |_worker_id| helper.workers_running_check(),
            |_state, i| {
                helper.workers_running_check()?;
                if i == n {
                    bail!("None shall pass!")
                } else {
                    Ok(())
                }
            },
            || Ok(Continue::Continue),
            || helper.finished_callback(),
            worker_count,
        );

        let msg = format!("{}", result.err().unwrap());
        assert!(msg == "None shall pass!");
    }

    /// Checks that the iteration stops when background function returns Stop.
    #[proptest]
    fn error_from_background(worker_count: WorkerCount) {
        let helper = IterationCheckHelper::new();

        let result = parallel_for_each(
            0..,
            |_worker_id| helper.workers_running_check(),
            |_state, _i| helper.workers_running_check(),
            || {
                helper.workers_running_check()?;
                bail!("None shall pass!");
            },
            || helper.finished_callback(),
            worker_count,
        );

        let msg = format!("{}", result.err().unwrap());
        assert!(msg == "None shall pass!");
    }
}
