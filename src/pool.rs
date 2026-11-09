//! A tiny bounded worker pool built on `std::thread::scope`.

use std::sync::Mutex;
use std::thread;

/// Runs `job` over every item using at most `workers` threads at a time.
///
/// Results are returned in the same order as `items`, regardless of which job
/// finishes first. `job` receives the item's index and the item itself.
/// A panic inside a job propagates to the caller once all threads have joined.
pub fn run_bounded<T, R, F>(items: Vec<T>, workers: usize, job: F) -> Vec<R>
where
    T: Send,
    R: Send,
    F: Fn(usize, T) -> R + Sync,
{
    let total = items.len();
    let workers = workers.clamp(1, total.max(1));

    let queue = Mutex::new(items.into_iter().enumerate());
    let results: Mutex<Vec<Option<R>>> = Mutex::new((0..total).map(|_| None).collect());

    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                // The guard is dropped at the end of this statement, so the
                // lock is never held while a job runs.
                let next = queue.lock().unwrap().next();
                match next {
                    Some((index, item)) => {
                        let result = job(index, item);
                        results.lock().unwrap()[index] = Some(result);
                    }
                    None => break,
                }
            });
        }
    });

    results
        .into_inner()
        .unwrap()
        .into_iter()
        .map(|r| r.expect("every job produces a result"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    #[test]
    fn preserves_input_order() {
        let items: Vec<u64> = (0..20).collect();
        let out = run_bounded(items, 4, |_, n| {
            // Reverse-ish sleep so later items finish first.
            thread::sleep(Duration::from_millis(20 - n));
            n * 2
        });
        assert_eq!(out, (0..20).map(|n| n * 2).collect::<Vec<_>>());
    }

    #[test]
    fn never_exceeds_worker_limit() {
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        run_bounded((0..16).collect::<Vec<_>>(), 3, |_, _| {
            let now = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            thread::sleep(Duration::from_millis(15));
            active.fetch_sub(1, Ordering::SeqCst);
        });
        assert!(peak.load(Ordering::SeqCst) <= 3);
        assert!(peak.load(Ordering::SeqCst) >= 2, "work should overlap");
    }

    #[test]
    fn handles_empty_input() {
        let out: Vec<u8> = run_bounded(Vec::<u8>::new(), 4, |_, n| n);
        assert!(out.is_empty());
    }
}
