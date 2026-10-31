use std::time::Instant;

fn main() {
    let size = 64 * 1024 * 1024; // 64 MiB
    let buffer = vec![0xABu8; size];

    println!("Benchmarking RollingChecksum::calculate on 64 MiB buffer...");

    // Warm-up
    let _ = ssync_core::checksum::RollingChecksum::calculate(&buffer[..1024 * 1024]);

    let iterations = 10;
    let start = Instant::now();
    for _ in 0..iterations {
        let cs = ssync_core::checksum::RollingChecksum::calculate(&buffer);
        std::hint::black_box(cs);
    }
    let elapsed = start.elapsed();

    let total_bytes = (size as f64) * (iterations as f64);
    let gib = total_bytes / (1024.0 * 1024.0 * 1024.0);
    let seconds = elapsed.as_secs_f64();
    let gib_per_sec = gib / seconds;

    println!(
        "Processed {:.2} GiB in {:.4} s ({:.2} GiB/s)",
        gib, seconds, gib_per_sec
    );
}
