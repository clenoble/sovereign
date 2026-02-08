mod backend;
mod benchmark;
mod llm;

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;

use backend::ModelBackend;
use benchmark::{get_rss_mb, get_vram_used_mb, print_results, time_it, BenchmarkResult};
use llm::LlamaCppBackend;

#[derive(Parser)]
#[command(name = "spike-3-llm-inference")]
#[command(about = "Validate llama.cpp FFI: load, infer, unload GGUF models from Rust")]
struct Cli {
    /// Directory containing GGUF model files
    #[arg(long, default_value = "./models")]
    model_dir: PathBuf,

    /// Number of GPU layers to offload (99 = all)
    #[arg(long, default_value = "99")]
    n_gpu_layers: u32,

    /// Context size in tokens
    #[arg(long, default_value = "2048")]
    n_ctx: u32,
}

const MODEL_3B: &str = "qwen2.5-3b-instruct-q4_k_m.gguf";
const MODEL_7B: &str = "qwen2.5-7b-instruct-q4_k_m-00001-of-00002.gguf";

const CLASSIFICATION_PROMPT: &str =
    "<|im_start|>user\nClassify this as positive or negative: \"I love this product\"\nRespond with one word.<|im_end|>\n<|im_start|>assistant\n";

const GENERATION_PROMPT: &str =
    "<|im_start|>user\nExplain what an operating system kernel does in 2 sentences.<|im_end|>\n<|im_start|>assistant\n";

fn run_model_benchmark(
    model_path: &str,
    model_name: &str,
    n_gpu_layers: u32,
    n_ctx: u32,
    load_target: Duration,
    infer_target: Duration,
) -> Result<(Vec<BenchmarkResult>, f64, f64)> {
    println!("\n--- {} ---\n", model_name);

    let rss_before = get_rss_mb();
    let vram_before = get_vram_used_mb();
    println!(
        "  Before load: RSS = {:.0} MB, VRAM = {:.0} MB",
        rss_before, vram_before
    );

    // Load model
    println!("  Loading model...");
    let (llm, load_time) = time_it(|| LlamaCppBackend::load(model_path, n_gpu_layers, n_ctx));
    let mut llm = llm.context("Model load failed")?;
    println!("  Load time: {:.2}s", load_time.as_secs_f64());

    let vram_loaded = get_vram_used_mb();
    println!(
        "  After load:  RSS = {:.0} MB, VRAM = {:.0} MB (+{:.0} MB VRAM)",
        get_rss_mb(),
        vram_loaded,
        vram_loaded - vram_before
    );

    // Inference 1: warmup (first call includes CUDA kernel compilation)
    println!("\n  Inference 1 (warmup): Classification (max 10 tokens)");
    let (result, warmup_time) = time_it(|| llm.generate_silent(CLASSIFICATION_PROMPT, 10));
    result?;
    println!("  Warmup time: {:.0}ms (includes CUDA kernel init)", warmup_time.as_millis());

    // Inference 2: generation (longer output, benchmarked)
    println!("\n  Inference 2: Generation (max 100 tokens)");
    print!("  Output: ");
    let (result, gen_time) = time_it(|| llm.generate(GENERATION_PROMPT, 100));
    result?;
    println!("  Generation time: {:.2}s", gen_time.as_secs_f64());

    // Inference 3: classification (warm, this is the benchmarked latency)
    println!("\n  Inference 3: Classification (max 10 tokens)");
    let (result, infer_time) = time_it(|| llm.generate_silent(CLASSIFICATION_PROMPT, 10));
    result?;
    println!("  Inference time: {:.0}ms", infer_time.as_millis());

    // Measure VRAM while model is still loaded for accurate delta
    let _vram_pre_unload = get_vram_used_mb();

    // Unload
    println!("\n  Unloading model...");
    llm.unload();

    // Brief pause for GPU memory reclamation
    std::thread::sleep(Duration::from_millis(500));

    let rss_after = get_rss_mb();
    let vram_after = get_vram_used_mb();
    println!(
        "  After unload: RSS = {:.0} MB, VRAM = {:.0} MB",
        rss_after, vram_after
    );

    // Compare VRAM: before-load vs after-unload
    // CUDA allocator may retain some memory between phases; hot-swap test is definitive
    let vram_not_freed = (vram_after - vram_before).abs();
    let load_passed = load_time <= load_target;
    let infer_passed = infer_time <= infer_target;
    let mem_passed = vram_not_freed <= 200.0;

    println!(
        "  VRAM delta (before load → after unload): {:.0} MB",
        vram_not_freed
    );

    let results = vec![
        BenchmarkResult {
            name: format!("{} load time", model_name),
            value: format!("{:.2}s", load_time.as_secs_f64()),
            target: format!("< {}s", load_target.as_secs()),
            passed: load_passed,
        },
        BenchmarkResult {
            name: format!("{} inference (classification)", model_name),
            value: format!("{:.0}ms", infer_time.as_millis()),
            target: format!("< {}ms", infer_target.as_millis()),
            passed: infer_passed,
        },
        BenchmarkResult {
            name: format!("{} VRAM reclaimed after unload", model_name),
            value: format!("{:.0} MB delta", vram_not_freed),
            target: "< 200 MB".to_string(),
            passed: mem_passed,
        },
    ];

    Ok((results, vram_before, vram_after))
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    println!("========================================");
    println!("  Spike 3: llama.cpp FFI Benchmark");
    println!("========================================");
    println!("  Model dir: {}", cli.model_dir.display());
    println!("  GPU layers: {}", cli.n_gpu_layers);
    println!("  Context size: {}", cli.n_ctx);

    let model_3b_path = cli.model_dir.join(MODEL_3B);
    let model_7b_path = cli.model_dir.join(MODEL_7B);

    let model_3b_str = model_3b_path
        .to_str()
        .context("Invalid model path")?
        .to_string();
    let model_7b_str = model_7b_path
        .to_str()
        .context("Invalid model path")?
        .to_string();

    let mut all_results: Vec<BenchmarkResult> = Vec::new();

    // Phase 1: 3B model benchmark
    println!("\n╔══════════════════════════════════════╗");
    println!("║  Phase 1: 3B Router Model Benchmark  ║");
    println!("╚══════════════════════════════════════╝");

    let (results_3b, _, _) = run_model_benchmark(
        &model_3b_str,
        "3B (router)",
        cli.n_gpu_layers,
        cli.n_ctx,
        Duration::from_secs(10),
        Duration::from_millis(500),
    )?;
    all_results.extend(results_3b);

    // Phase 2: 7B model benchmark
    println!("\n╔══════════════════════════════════════════╗");
    println!("║  Phase 2: 7B Reasoning Model Benchmark   ║");
    println!("╚══════════════════════════════════════════╝");

    let (results_7b, _, _) = run_model_benchmark(
        &model_7b_str,
        "7B (reasoning)",
        cli.n_gpu_layers,
        cli.n_ctx,
        Duration::from_secs(10),
        Duration::from_millis(2000),
    )?;
    all_results.extend(results_7b);

    // Phase 3: Hot-swap test
    println!("\n╔══════════════════════════════════════╗");
    println!("║  Phase 3: Hot-Swap Test (3B → 7B)    ║");
    println!("╚══════════════════════════════════════╝");

    let vram_baseline = get_vram_used_mb();
    println!("\n  VRAM baseline: {:.0} MB", vram_baseline);

    // Load 3B, infer, unload
    println!("\n  Step 1: Load 3B...");
    let mut llm_3b = LlamaCppBackend::load(&model_3b_str, cli.n_gpu_layers, cli.n_ctx)?;
    println!("  VRAM after 3B load: {:.0} MB", get_vram_used_mb());

    println!("  Inference with 3B...");
    let _ = llm_3b.generate_silent(CLASSIFICATION_PROMPT, 10)?;

    println!("  Unloading 3B...");
    llm_3b.unload();
    std::thread::sleep(Duration::from_millis(500));
    let vram_after_3b_unload = get_vram_used_mb();
    println!("  VRAM after 3B unload: {:.0} MB", vram_after_3b_unload);

    // Load 7B, infer, unload
    println!("\n  Step 2: Load 7B...");
    let mut llm_7b = LlamaCppBackend::load(&model_7b_str, cli.n_gpu_layers, cli.n_ctx)?;
    println!("  VRAM after 7B load: {:.0} MB", get_vram_used_mb());

    println!("  Inference with 7B...");
    let _ = llm_7b.generate_silent(GENERATION_PROMPT, 50)?;

    println!("  Unloading 7B...");
    llm_7b.unload();
    std::thread::sleep(Duration::from_millis(500));
    let vram_final = get_vram_used_mb();
    println!("  VRAM after 7B unload: {:.0} MB", vram_final);

    let hotswap_delta = (vram_final - vram_baseline).abs();
    let hotswap_passed = hotswap_delta <= 100.0;

    all_results.push(BenchmarkResult {
        name: "Hot-swap 3B→7B (no OOM, VRAM reclaimed)".to_string(),
        value: format!("{:.0} MB delta", hotswap_delta),
        target: "< 100 MB".to_string(),
        passed: hotswap_passed,
    });

    // Print consolidated results
    println!("\n╔══════════════════════════════════════╗");
    println!("║  CONSOLIDATED RESULTS                ║");
    println!("╚══════════════════════════════════════╝");
    print_results(&all_results);

    Ok(())
}
