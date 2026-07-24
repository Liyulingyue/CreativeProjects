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
                    n_complete: AtomicI32::new(0),
                    epoch: AtomicU32::new(0),
                    shutdown: AtomicBool::new(true),
                }),
                threads: Vec::new(),
            };
        }

        let inner = Arc::new(Inner {
            call_fn: AtomicUsize::new(0),
            call_data: AtomicUsize::new(0),
            n_complete: AtomicI32::new(0),
            epoch: AtomicU32::new(1),
            shutdown: AtomicBool::new(false),
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

        let call_fn = call_closure::<F> as usize;
        let target = self.inner.n_complete.load(Ordering::Acquire) + self.n_threads as i32;

        self.inner.call_fn.store(call_fn, Ordering::Release);
        self.inner.call_data.store(data_ptr, Ordering::Release);
        self.inner.epoch.fetch_add(1, Ordering::SeqCst);

        unsafe { call_closure::<F>(0, self.n_threads, data_ptr); }
        self.inner.n_complete.fetch_add(1, Ordering::SeqCst);

        while self.inner.n_complete.load(Ordering::Acquire) < target {
            std::hint::spin_loop();
        }

        unsafe { drop(Box::from_raw(data_ptr as *mut F)); }
    }

    pub fn next_chunk(&self) -> i32 {
        0
    }
}

impl Drop for ComputePool {
    fn drop(&mut self) {
        self.inner.shutdown.store(true, Ordering::Release);
        self.inner.epoch.fetch_add(1, Ordering::SeqCst);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

fn worker_loop(tid: usize, n_threads: usize, inner: &Inner) {
    let mut my_epoch: u32 = inner.epoch.load(Ordering::Acquire);
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
