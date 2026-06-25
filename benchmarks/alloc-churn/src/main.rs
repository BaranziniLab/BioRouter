//! Allocator A/B churn microbench.
//!
//! Mimics BioRouter's hot allocation pattern: every turn the full conversation
//! transcript is re-read from SQLite into a `Vec<Message>` and cloned 2-3x
//! (`get_conversation` + `fix_conversation`), then dropped. Over a long session
//! across many worker threads this churns the allocator hard. The metric that
//! matters is how much memory the allocator *returns to the OS* after the
//! transcripts are freed — that is exactly where a tuned jemalloc beats the
//! system allocator's retained arenas.
use std::time::Duration;

#[cfg(feature = "jemalloc")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

#[cfg(feature = "jemalloc")]
fn tune() {
    use tikv_jemalloc_ctl::raw;
    unsafe {
        let _ = raw::write(b"arena.4096.dirty_decay_ms\0", 1000_isize);
        let _ = raw::write(b"arena.4096.muzzy_decay_ms\0", 1000_isize);
        let _ = raw::write(b"arenas.dirty_decay_ms\0", 1000_isize);
        let _ = raw::write(b"arenas.muzzy_decay_ms\0", 1000_isize);
    }
}
#[cfg(not(feature = "jemalloc"))]
fn tune() {}

fn rss_mb() -> f64 {
    memory_stats::memory_stats()
        .map(|s| s.physical_mem as f64 / 1_048_576.0)
        .unwrap_or(0.0)
}

/// ~2 KB owned String, like one serialized transcript entry.
fn make_message(i: usize) -> String {
    format!(
        "{{\"role\":\"assistant\",\"id\":{i},\"content\":\"{}\"}}",
        "x".repeat(2000)
    )
}

/// One session: transcript grows over `turns`, each turn re-clones it twice.
fn churn_session(turns: usize) {
    let mut convo: Vec<String> = Vec::new();
    for t in 0..turns {
        for k in 0..4 {
            convo.push(make_message(t * 4 + k));
        }
        let c1 = convo.clone(); // get_conversation -> Vec<Message>
        let c2 = c1.clone(); // fix_conversation double-clone
        std::hint::black_box((&c1, &c2));
        drop(c1);
        drop(c2);
    }
    std::hint::black_box(&convo); // session end -> whole transcript dropped
}

fn main() {
    tune();
    let threads = 6;
    let sessions = 40;
    let turns = 60;
    let mut peak = rss_mb();
    for _ in 0..sessions {
        let handles: Vec<_> = (0..threads)
            .map(|_| std::thread::spawn(move || churn_session(turns)))
            .collect();
        for h in handles {
            let _ = h.join();
        }
        let r = rss_mb();
        if r > peak {
            peak = r;
        }
    }
    let after = rss_mb();
    // Let time-based decay return freed pages; a little light activity gives the
    // allocator a chance to run purge maintenance (no background thread on macOS).
    std::thread::sleep(Duration::from_millis(1500));
    for _ in 0..50_000 {
        std::hint::black_box(vec![0u8; 64]);
    }
    std::thread::sleep(Duration::from_millis(1500));
    let settled = rss_mb();
    println!("peak_rss_mb={peak:.1} after_churn_rss_mb={after:.1} settled_rss_mb={settled:.1}");
}
