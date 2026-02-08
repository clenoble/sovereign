//! Spike 2: SurrealDB Embedded Performance Benchmark
//!
//! Validates that embedded SurrealDB meets performance requirements:
//! - Single document fetch: < 5ms
//! - Graph traversal (2 hops, 10 results): < 50ms
//! - Bulk insert 50K documents: < 30s
//!
//! Usage:
//!   cargo run --release                    # In-memory benchmark
//!   cargo run --release -- --persistent    # RocksDB persistent benchmark

mod benchmark;
mod generator;
mod schema;

use benchmark::{Benchmark, BenchmarkResults};
use generator::DataGenerator;
use surrealdb::engine::local::{Db, Mem, RocksDb};
use surrealdb::Surreal;
use std::env;
use std::time::Instant;

const DOC_COUNT: usize = 50_000;
const THREAD_COUNT: usize = 8;
const BATCH_SIZE: usize = 1000;
const RELATIONSHIPS_PER_DOC: usize = 3;

async fn run_benchmark_mem() -> anyhow::Result<()> {
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║  Spike 2: SurrealDB Embedded Benchmark (IN-MEMORY)       ║");
    println!("╚═══════════════════════════════════════════════════════════╝\n");

    // Connect to in-memory database
    println!("📦 Connecting to in-memory SurrealDB...");
    let db: Surreal<Db> = Surreal::new::<Mem>(()).await?;
    db.use_ns("test").use_db("test").await?;
    println!("✓ Connected\n");

    run_benchmark_common(db).await
}

async fn run_benchmark_persistent() -> anyhow::Result<()> {
    println!("╔═══════════════════════════════════════════════════════════╗");
    println!("║  Spike 2: SurrealDB Embedded Benchmark (ROCKSDB)         ║");
    println!("╚═══════════════════════════════════════════════════════════╝\n");

    // Clean up any existing database
    let db_path = "./benchmark_db";
    if std::path::Path::new(db_path).exists() {
        std::fs::remove_dir_all(db_path)?;
    }

    // Connect to RocksDB persistent database
    println!("📦 Connecting to RocksDB SurrealDB at {}...", db_path);
    let db: Surreal<Db> = Surreal::new::<RocksDb>(db_path).await?;
    db.use_ns("test").use_db("test").await?;
    println!("✓ Connected\n");

    let result = run_benchmark_common(db).await;

    // Cleanup
    println!("\n🗑️  Cleaning up database files...");
    std::fs::remove_dir_all(db_path)?;

    result
}

async fn run_benchmark_common(db: Surreal<Db>) -> anyhow::Result<()> {
    let mut generator = DataGenerator::new();

    // ── Phase 1: Generate and insert threads ────────────────────────────────
    println!("📊 Phase 1: Creating {} threads...", THREAD_COUNT);
    let threads = generator.generate_threads(THREAD_COUNT);
    let thread_ids = DataGenerator::insert_threads(&db, threads).await?;
    println!("✓ Created {} threads\n", thread_ids.len());

    // ── Phase 2: Bulk insert 50K documents ───────────────────────────────────
    println!(
        "📊 Phase 2: Generating and inserting {} documents...",
        DOC_COUNT
    );
    println!("   Batch size: {}", BATCH_SIZE);

    let documents = generator.generate_documents(DOC_COUNT, &thread_ids);

    let bulk_insert_start = Instant::now();
    let doc_ids = DataGenerator::insert_documents(&db, documents, BATCH_SIZE).await?;
    let bulk_insert_elapsed = bulk_insert_start.elapsed();

    let bulk_insert_ms = bulk_insert_elapsed.as_secs_f64() * 1000.0;
    let bulk_insert_s = bulk_insert_ms / 1000.0;

    println!("✓ Inserted {} documents", doc_ids.len());
    println!(
        "  Time: {:.2} s ({:.0} docs/s)\n",
        bulk_insert_s,
        DOC_COUNT as f64 / bulk_insert_s
    );

    // ── Phase 3: Create relationships ────────────────────────────────────────
    println!("📊 Phase 3: Creating relationships...");
    println!(
        "   Target: ~{} relationships ({} per doc)",
        DOC_COUNT * RELATIONSHIPS_PER_DOC,
        RELATIONSHIPS_PER_DOC
    );

    let rel_start = Instant::now();
    let rel_count = generator
        .create_relationships(&db, &doc_ids, RELATIONSHIPS_PER_DOC)
        .await?;
    let rel_elapsed = rel_start.elapsed();

    println!("✓ Created {} relationships", rel_count);
    println!("  Time: {:.2} s\n", rel_elapsed.as_secs_f64());

    // ── Phase 4: Run performance benchmarks ──────────────────────────────────
    println!("📊 Phase 4: Running performance benchmarks...");
    let bench = Benchmark::new(db);
    let mut results = bench.run_all(&doc_ids).await?;

    // Add bulk insert timing
    results.bulk_insert_50k_ms = bulk_insert_ms;

    // Print results
    results.print_report();

    // ── Summary ──────────────────────────────────────────────────────────────
    println!("📈 SUMMARY");
    println!("  Total documents: {}", doc_ids.len());
    println!("  Total relationships: {}", rel_count);
    println!("  Database size: {} records", doc_ids.len() + thread_ids.len());
    println!();

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    let persistent = args.iter().any(|arg| arg == "--persistent");

    if persistent {
        run_benchmark_persistent().await
    } else {
        run_benchmark_mem().await
    }
}
