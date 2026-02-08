use std::fs;
use std::process::Command;
use std::time::{Duration, Instant};

pub struct BenchmarkResult {
    pub name: String,
    pub value: String,
    pub target: String,
    pub passed: bool,
}

pub fn get_rss_mb() -> f64 {
    let status = fs::read_to_string("/proc/self/status").unwrap_or_default();
    for line in status.lines() {
        if line.starts_with("VmRSS:") {
            let kb: f64 = line
                .split_whitespace()
                .nth(1)
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            return kb / 1024.0;
        }
    }
    0.0
}

pub fn get_vram_used_mb() -> f64 {
    let output = Command::new("nvidia-smi")
        .args(["--query-gpu=memory.used", "--format=csv,noheader,nounits"])
        .output();
    match output {
        Ok(out) => {
            let s = String::from_utf8_lossy(&out.stdout);
            s.trim().parse().unwrap_or(0.0)
        }
        Err(_) => 0.0,
    }
}

pub fn time_it<F, T>(f: F) -> (T, Duration)
where
    F: FnOnce() -> T,
{
    let start = Instant::now();
    let result = f();
    (result, start.elapsed())
}

pub fn print_results(results: &[BenchmarkResult]) {
    let name_w = 40;
    let val_w = 14;
    let tgt_w = 12;
    let pass_w = 6;
    let total_w = name_w + val_w + tgt_w + pass_w + 7; // separators

    println!();
    println!("{}", "=".repeat(total_w));
    println!(
        " {:name_w$} | {:>val_w$} | {:>tgt_w$} | {:>pass_w$}",
        "Benchmark",
        "Result",
        "Target",
        "Pass",
        name_w = name_w,
        val_w = val_w,
        tgt_w = tgt_w,
        pass_w = pass_w,
    );
    println!("{}", "-".repeat(total_w));

    let mut all_passed = true;
    for r in results {
        let icon = if r.passed { "\u{2713}" } else { "\u{2717}" };
        if !r.passed {
            all_passed = false;
        }
        println!(
            " {:name_w$} | {:>val_w$} | {:>tgt_w$} | {:>pass_w$}",
            r.name,
            r.value,
            r.target,
            icon,
            name_w = name_w,
            val_w = val_w,
            tgt_w = tgt_w,
            pass_w = pass_w,
        );
    }
    println!("{}", "=".repeat(total_w));

    if all_passed {
        println!("\n  ALL BENCHMARKS PASSED");
    } else {
        println!("\n  SOME BENCHMARKS FAILED");
    }
    println!();
}
