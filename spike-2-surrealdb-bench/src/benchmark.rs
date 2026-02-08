//! Benchmark suite for SurrealDB performance validation

use crate::schema::Document;
use surrealdb::engine::local::Db;
use surrealdb::Surreal;
use std::time::Instant;

pub struct BenchmarkResults {
    pub single_fetch_ms: f64,
    pub graph_traversal_2hop_ms: f64,
    pub bulk_insert_50k_ms: f64,
    pub search_by_title_ms: f64,
    pub thread_query_ms: f64,
}

impl BenchmarkResults {
    pub fn print_report(&self) {
        println!("\n╔═══════════════════════════════════════════════════════════╗");
        println!("║           SURREALDB BENCHMARK RESULTS                     ║");
        println!("╠═══════════════════════════════════════════════════════════╣");
        println!("║ Test                          │ Result    │ Target  │ Pass ║");
        println!("╠═══════════════════════════════════════════════════════════╣");

        self.print_result(
            "Single document fetch",
            self.single_fetch_ms,
            5.0,
            "ms",
        );
        self.print_result(
            "Graph traversal (2 hops)",
            self.graph_traversal_2hop_ms,
            50.0,
            "ms",
        );
        self.print_result(
            "Bulk insert 50K docs",
            self.bulk_insert_50k_ms / 1000.0,
            30.0,
            "s",
        );
        self.print_result(
            "Search by title",
            self.search_by_title_ms,
            100.0,
            "ms",
        );
        self.print_result(
            "Thread documents query",
            self.thread_query_ms,
            50.0,
            "ms",
        );

        println!("╚═══════════════════════════════════════════════════════════╝\n");

        // Overall assessment
        let all_pass = self.single_fetch_ms < 5.0
            && self.graph_traversal_2hop_ms < 50.0
            && self.bulk_insert_50k_ms < 30000.0
            && self.search_by_title_ms < 100.0
            && self.thread_query_ms < 50.0;

        if all_pass {
            println!("✅ ALL BENCHMARKS PASSED — SurrealDB meets performance requirements");
        } else {
            println!("❌ SOME BENCHMARKS FAILED — Consider SQLite fallback");
        }
    }

    fn print_result(&self, name: &str, value: f64, target: f64, unit: &str) {
        let pass = value < target;
        let status = if pass { "✓" } else { "✗" };
        println!(
            "║ {:<29} │ {:>6.2} {} │ {:>4} {} │  {}   ║",
            name, value, unit, target as i32, unit, status
        );
    }
}

pub struct Benchmark {
    db: Surreal<Db>,
}

impl Benchmark {
    pub fn new(db: Surreal<Db>) -> Self {
        Self { db }
    }

    /// Run all benchmarks
    pub async fn run_all(&self, doc_ids: &[String]) -> anyhow::Result<BenchmarkResults> {
        println!("\n🔬 Running benchmarks...\n");

        let single_fetch_ms = self.bench_single_fetch(doc_ids).await?;
        let graph_traversal_2hop_ms = self.bench_graph_traversal_2hop(doc_ids).await?;
        let search_by_title_ms = self.bench_search_by_title().await?;
        let thread_query_ms = self.bench_thread_query().await?;

        // Bulk insert is measured during data generation
        let bulk_insert_50k_ms = 0.0; // Will be filled in by main

        Ok(BenchmarkResults {
            single_fetch_ms,
            graph_traversal_2hop_ms,
            bulk_insert_50k_ms,
            search_by_title_ms,
            thread_query_ms,
        })
    }

    /// Benchmark: Single document fetch
    /// Target: < 5ms
    async fn bench_single_fetch(&self, doc_ids: &[String]) -> anyhow::Result<f64> {
        if doc_ids.is_empty() {
            return Ok(0.0);
        }

        let doc_id_str = &doc_ids[doc_ids.len() / 2]; // Pick middle document
        // Parse "document:xyz" into ("document", "xyz") for single-record select
        let (table, id) = doc_id_str.split_once(':').unwrap();

        // Warmup
        let _: Option<Document> = self.db.select((table, id)).await?;

        // Benchmark (average of 100 runs)
        let iterations = 100;
        let start = Instant::now();

        for _ in 0..iterations {
            let _: Option<Document> = self.db.select((table, id)).await?;
        }

        let elapsed = start.elapsed();
        let avg_ms = elapsed.as_secs_f64() * 1000.0 / iterations as f64;

        println!("  Single fetch: {:.3} ms (avg of {} runs)", avg_ms, iterations);

        Ok(avg_ms)
    }

    /// Benchmark: Graph traversal (2 hops, 10 results)
    /// Target: < 50ms
    async fn bench_graph_traversal_2hop(&self, doc_ids: &[String]) -> anyhow::Result<f64> {
        if doc_ids.is_empty() {
            return Ok(0.0);
        }

        let doc_id = &doc_ids[doc_ids.len() / 3];

        // Query: Start from a document, traverse relationships 2 hops
        let query = format!(
            "SELECT ->related_to->document->related_to->document.title FROM {} LIMIT 10",
            doc_id
        );

        // Warmup
        let _ = self.db.query(&query).await?;

        // Benchmark (average of 50 runs)
        let iterations = 50;
        let start = Instant::now();

        for _ in 0..iterations {
            let mut result = self.db.query(&query).await?;
            let _: Vec<serde_json::Value> = result.take(0)?;
        }

        let elapsed = start.elapsed();
        let avg_ms = elapsed.as_secs_f64() * 1000.0 / iterations as f64;

        println!(
            "  Graph traversal (2 hops): {:.3} ms (avg of {} runs)",
            avg_ms, iterations
        );

        Ok(avg_ms)
    }

    /// Benchmark: Search by title (full-text)
    /// Target: < 100ms
    async fn bench_search_by_title(&self) -> anyhow::Result<f64> {
        let query = "SELECT * FROM document WHERE title CONTAINS 'Meeting' LIMIT 50";

        // Warmup
        let _ = self.db.query(query).await?;

        // Benchmark (average of 50 runs)
        let iterations = 50;
        let start = Instant::now();

        for _ in 0..iterations {
            let mut result = self.db.query(query).await?;
            let _: Vec<Document> = result.take(0)?;
        }

        let elapsed = start.elapsed();
        let avg_ms = elapsed.as_secs_f64() * 1000.0 / iterations as f64;

        println!(
            "  Search by title: {:.3} ms (avg of {} runs)",
            avg_ms, iterations
        );

        Ok(avg_ms)
    }

    /// Benchmark: Query all documents in a thread
    /// Target: < 50ms
    async fn bench_thread_query(&self) -> anyhow::Result<f64> {
        let query = "SELECT * FROM document WHERE thread_id = 'thread:research' LIMIT 100";

        // Warmup
        let _ = self.db.query(query).await?;

        // Benchmark (average of 50 runs)
        let iterations = 50;
        let start = Instant::now();

        for _ in 0..iterations {
            let mut result = self.db.query(query).await?;
            let _: Vec<Document> = result.take(0)?;
        }

        let elapsed = start.elapsed();
        let avg_ms = elapsed.as_secs_f64() * 1000.0 / iterations as f64;

        println!(
            "  Thread query: {:.3} ms (avg of {} runs)",
            avg_ms, iterations
        );

        Ok(avg_ms)
    }
}
