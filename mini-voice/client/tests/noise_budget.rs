//! Isolated allocator check: one test, no devices or network.
use mini_client::noise_cleaner::NoiseCleaner;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::atomic::{AtomicUsize, Ordering},
    time::Instant,
};
struct Meter;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static ALLOCS: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        let p = System.alloc(l);
        if !p.is_null() {
            LIVE.fetch_add(l.size(), Ordering::Relaxed);
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        LIVE.fetch_sub(l.size(), Ordering::Relaxed);
        System.dealloc(p, l);
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, size: usize) -> *mut u8 {
        let q = System.realloc(p, l, size);
        if !q.is_null() {
            LIVE.fetch_sub(l.size(), Ordering::Relaxed);
            LIVE.fetch_add(size, Ordering::Relaxed);
            ALLOCS.fetch_add(1, Ordering::Relaxed);
        }
        q
    }
}
#[global_allocator]
static METER: Meter = Meter;
#[test]
fn bounded_state_no_steady_allocations_and_noise_reduction() {
    let baseline = LIVE.load(Ordering::Relaxed);
    let mut cleaner = NoiseCleaner::new();
    let mut frame = [0.; mini_shared::FRAME];
    cleaner.process(&mut frame, true);
    let retained = LIVE.load(Ordering::Relaxed).saturating_sub(baseline);
    assert!(
        retained < 256 * 1024,
        "unexpected denoiser heap: {retained}"
    );
    let before = ALLOCS.load(Ordering::Relaxed);
    let start = Instant::now();
    let mut seed = 42u32;
    let mut input_power = 0f64;
    let mut output_power = 0f64;
    for block in 0..100 {
        for sample in &mut frame {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            *sample = (seed as f32 / u32::MAX as f32 * 2. - 1.) * 0.02;
            if block >= 50 {
                input_power += (*sample as f64).powi(2);
            }
        }
        cleaner.process(&mut frame, true);
        if block >= 50 {
            output_power += frame.iter().map(|s| (*s as f64).powi(2)).sum::<f64>();
        }
    }
    let elapsed = start.elapsed();
    let allocations = ALLOCS.load(Ordering::Relaxed) - before;
    assert_eq!(allocations, 0, "frame path must reuse FFT/state buffers");
    assert!(
        output_power < input_power * 0.85,
        "stationary noise not reduced: power ratio {}",
        output_power / input_power
    );
    println!("RNNoise retained heap (including FFT cache): {retained} bytes; 2s audio in {elapsed:?}; steady frame allocations: {allocations}; noise RMS ratio: {}", (output_power/input_power).sqrt());
}
