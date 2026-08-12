use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicUsize, Ordering};
use std::sync::Arc;

pub struct ComputePool {
    n_threads: usize,
    inner: Arc<Inner>,
    threads: Vec<std::thread::JoinHandle<()>>,
}

struct Inner {
    call_fn: AtomicUsize,
    call_data: AtomicUsize,
    n_complete: AtomicI32,
    epoch: AtomicU32,
    shutdown: AtomicBool,
    chunk_counter: AtomicI32,
    chunk_barrier: std::sync::atomic::AtomicUsize,
    chunk_n_chunks: std::sync::atomic::AtomicI32,
}

type CallFn = unsafe fn(usize, usize, usize);

impl ComputePool {
    pub fn new(n_threads: usize) -> Self {
        if n_threads <= 1 {
            return ComputePool {
                n_threads: 1,
                inner: Arc::new(Inner {
                    call_fn: AtomicUsize::new(0),
                    call_data: AtomicUsize::new(0),
                    n_complete: AtomicI32::new(1),
                    epoch: AtomicU32::new(0),
                    shutdown: AtomicBool::new(true),
                    chunk_counter: AtomicI32::new(0),
                    chunk_barrier: AtomicUsize::new(0),
                    chunk_n_chunks: AtomicI32::new(0),
                }),
                threads: Vec::new(),
            };
        }

        let inner = Arc::new(Inner {
            call_fn: AtomicUsize::new(0),
            call_data: AtomicUsize::new(0),
            n_complete: AtomicI32::new(n_threads as i32),
            epoch: AtomicU32::new(0),
            shutdown: AtomicBool::new(false),
            chunk_counter: AtomicI32::new(0),
            chunk_barrier: AtomicUsize::new(0),
            chunk_n_chunks: AtomicI32::new(0),
        });

        let mut threads = Vec::with_capacity(n_threads - 1);
        let start_barrier = Arc::new(std::sync::Barrier::new(n_threads));
        for tid in 1..n_threads {
            let inner = inner.clone();
            let nt = n_threads;
            let barrier = start_barrier.clone();
            threads.push(std::thread::spawn(move || {
                barrier.wait();
                worker_loop(tid, nt, &inner);
            }));
        }

        start_barrier.wait();

        ComputePool { n_threads, inner, threads }
    }

    pub fn n_threads(&self) -> usize { self.n_threads }

    pub fn compute<F: Fn(usize, usize)>(&self, f: F) {
        if self.n_threads <= 1 {
            f(0, 1);
            return;
        }

        let boxed = Box::new(f);
        let data_ptr = Box::into_raw(boxed) as usize;

        unsafe fn call_closure<F: Fn(usize, usize)>(ith: usize, nth: usize, data: usize) {
            let f = &*(data as *const F);
            f(ith, nth);
        }

        let call_fn = call_closure::<F> as *const () as usize;

        // Wait for all workers to be idle from previous compute
        while self.inner.n_complete.load(Ordering::Acquire) < self.n_threads as i32 {
            std::hint::spin_loop();
        }
        std::sync::atomic::fence(Ordering::SeqCst);

        // Reset completion counter before publishing new work
        self.inner.n_complete.store(0, Ordering::Relaxed);
        self.inner.call_fn.store(call_fn, Ordering::Relaxed);
        self.inner.call_data.store(data_ptr, Ordering::Relaxed);
        std::sync::atomic::fence(Ordering::SeqCst);

        // Publish: epoch increment signals workers to start
        self.inner.epoch.fetch_add(1, Ordering::SeqCst);

        // Main thread does its share
        unsafe { call_closure::<F>(0, self.n_threads, data_ptr); }
        self.inner.n_complete.fetch_add(1, Ordering::SeqCst);

        // Wait for all threads to finish
        while self.inner.n_complete.load(Ordering::Acquire) < self.n_threads as i32 {
            std::hint::spin_loop();
        }
        std::sync::atomic::fence(Ordering::SeqCst);

        unsafe { drop(Box::from_raw(data_ptr as *mut F)); }
    }

    pub fn compute_with_chunks<F: Fn(usize, i32) + Send + Sync + 'static>(&self, n_chunks: i32, f: F) {
        let f = Arc::new(f);
        if self.n_threads <= 1 {
            let mut chunk_id = 0;
            while chunk_id < n_chunks {
                f(0, chunk_id);
                chunk_id += 1;
            }
            return;
        }

        while self.inner.n_complete.load(Ordering::Acquire) < self.n_threads as i32 {
            std::hint::spin_loop();
        }
        std::sync::atomic::fence(Ordering::SeqCst);

        self.inner.n_complete.store(0, Ordering::Relaxed);

        let barrier = Arc::new(std::sync::Barrier::new(self.n_threads));
        let mut handles = Vec::with_capacity(self.n_threads - 1);
        for tid in 1..self.n_threads {
            let barrier = barrier.clone();
            let inner = self.inner.clone();
            let f = f.clone();
            let handle = std::thread::spawn(move || {
                barrier.wait();
                let mut chunk_id = inner.chunk_counter.fetch_add(1, Ordering::Relaxed);
                loop {
                    if chunk_id >= n_chunks { break; }
                    f(tid, chunk_id);
                    chunk_id = inner.chunk_counter.fetch_add(1, Ordering::Relaxed);
                }
            });
            handles.push(handle);
        }

        self.inner.call_fn.store(0, Ordering::Relaxed);
        self.inner.call_data.store(0, Ordering::Relaxed);
        self.inner.chunk_counter.store(self.n_threads as i32, Ordering::Relaxed);
        self.inner.chunk_barrier.store(
            Arc::as_ptr(&barrier) as usize,
            Ordering::Relaxed
        );
        self.inner.chunk_n_chunks.store(n_chunks, Ordering::Relaxed);
        std::sync::atomic::fence(Ordering::SeqCst);

        self.inner.epoch.fetch_add(1, Ordering::SeqCst);

        while self.inner.n_complete.load(Ordering::Acquire) < (self.n_threads - 1) as i32 {
            std::hint::spin_loop();
        }

        barrier.wait();

        let mut chunk_id = self.inner.chunk_counter.fetch_add(1, Ordering::Relaxed);
        loop {
            if chunk_id >= n_chunks { break; }
            f(0, chunk_id);
            chunk_id = self.inner.chunk_counter.fetch_add(1, Ordering::Relaxed);
        }

        self.inner.n_complete.fetch_add(1, Ordering::SeqCst);

        while self.inner.n_complete.load(Ordering::Acquire) < self.n_threads as i32 {
            std::hint::spin_loop();
        }
        std::sync::atomic::fence(Ordering::SeqCst);

        self.inner.call_fn.store(0, Ordering::Relaxed);
        self.inner.call_data.store(0, Ordering::Relaxed);
        self.inner.chunk_barrier.store(0, Ordering::Relaxed);
        self.inner.chunk_n_chunks.store(0, Ordering::Relaxed);

        for h in handles {
            let _ = h.join();
        }
    }

    pub fn next_chunk(&self) -> i32 {
        0
    }
}

impl Drop for ComputePool {
    fn drop(&mut self) {
        while self.inner.n_complete.load(Ordering::Acquire) < self.n_threads as i32 {
            std::hint::spin_loop();
        }
        self.inner.n_complete.store(0, Ordering::Relaxed);
        self.inner.shutdown.store(true, Ordering::Relaxed);
        std::sync::atomic::fence(Ordering::SeqCst);
        self.inner.epoch.fetch_add(1, Ordering::SeqCst);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

fn worker_loop(tid: usize, n_threads: usize, inner: &Inner) {
    let mut my_epoch: u32 = 0;
    loop {
        while inner.epoch.load(Ordering::Acquire) == my_epoch {
            if inner.shutdown.load(Ordering::Acquire) { return; }
            std::hint::spin_loop();
        }
        my_epoch = inner.epoch.load(Ordering::Acquire);
        if inner.shutdown.load(Ordering::Acquire) { return; }

        let call_fn = inner.call_fn.load(Ordering::Acquire);
        let call_data = inner.call_data.load(Ordering::Acquire);
        if call_fn != 0 {
            let f: CallFn = unsafe { std::mem::transmute(call_fn) };
            unsafe { f(tid, n_threads, call_data); }
        }

        inner.n_complete.fetch_add(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn single_thread_pool_drops_without_spinning() {
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::spawn(move || {
            let pool = ComputePool::new(1);
            pool.compute(|ith, nth| assert_eq!((ith, nth), (0, 1)));
            drop(pool);
            tx.send(()).unwrap();
        });

        rx.recv_timeout(Duration::from_secs(1))
            .expect("single-thread ComputePool::drop timed out");
        handle.join().unwrap();
    }
}
