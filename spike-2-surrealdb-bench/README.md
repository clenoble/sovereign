# Spike 2: SurrealDB Embedded Benchmark

Performance validation for embedded SurrealDB as the graph database backend for Sovereign OS.

## Acceptance Criteria

| Benchmark | Target | Description |
|-----------|--------|-------------|
| Single document fetch | < 5ms | Retrieve one document by ID |
| Graph traversal (2 hops) | < 50ms | Navigate relationships, return 10 results |
| Bulk insert 50K docs | < 30s | Initial data import performance |
| Search by title | < 100ms | Full-text search across documents |
| Thread query | < 50ms | Filter documents by thread |

## What This Tests

1. **Document CRUD performance** - Create, read 50K documents
2. **Graph relationship performance** - ~150K relationships (3 per doc)
3. **Query performance** - Graph traversal, search, filtering
4. **Storage engines** - In-memory (Mem) vs persistent (RocksDB)

## Data Model

- **Threads**: 8 threads (Research, Development, Design, etc.)
- **Documents**: 50K documents with:
  - Title, type, content
  - Thread assignment
  - Ownership flag (70% owned, 30% external)
  - Spatial coordinates (for canvas positioning)
- **Relationships**: ~150K edges between documents:
  - References, DerivedFrom, Continues, Contradicts, Supports
  - Strength value (0.0 - 1.0)

## Usage

### In-Memory Benchmark (Fastest)

```bash
cargo run --release
```

### Persistent RocksDB Benchmark

```bash
cargo run --release -- --persistent
```

### From WSL (Recommended for Performance)

```bash
# Copy to WSL filesystem first
cd ~
rm -rf spike-2-surrealdb-bench  # Clean previous copy
cp -r /mnt/nas/home/Current/Projets/03\ -\ user-centered\ OS/spike-2-surrealdb-bench .
cd spike-2-surrealdb-bench

# Run benchmark
cargo run --release
```

## Expected Output

```
╔═══════════════════════════════════════════════════════════╗
║           SURREALDB BENCHMARK RESULTS                     ║
╠═══════════════════════════════════════════════════════════╣
║ Test                          │ Result    │ Target  │ Pass ║
╠═══════════════════════════════════════════════════════════╣
║ Single document fetch         │   2.34 ms │    5 ms │  ✓   ║
║ Graph traversal (2 hops)      │  18.72 ms │   50 ms │  ✓   ║
║ Bulk insert 50K docs          │  12.45 s  │   30 s  │  ✓   ║
║ Search by title               │  32.18 ms │  100 ms │  ✓   ║
║ Thread documents query        │  15.43 ms │   50 ms │  ✓   ║
╚═══════════════════════════════════════════════════════════╝

✅ ALL BENCHMARKS PASSED — SurrealDB meets performance requirements
```

## Decision Gates

- **✅ All benchmarks pass**: Proceed with SurrealDB for Phase 1
- **❌ Some benchmarks fail**: Evaluate fallback to SQLite + JSONB with application-level graph traversal

## Architecture Notes

### SurrealDB Advantages

- **Native graph support**: `RELATE` statements for edges, arrow operators (`->`, `<-`) for traversal
- **Embedded mode**: No separate server process, simpler deployment
- **Schema flexibility**: Mix of structured and schemaless data
- **Multi-model**: Documents + graph + key-value in one DB

### Trade-offs

- **Maturity**: SurrealDB is younger than PostgreSQL/SQLite
- **Ecosystem**: Fewer third-party tools and integrations
- **Storage size**: RocksDB may use more disk than SQLite (LSM-tree overhead)

## Next Steps

After validating SurrealDB:

1. **Phase 0, Spike 3**: PyO3 + Model Loading validation
2. **Phase 1**: Build `sovereign-db` crate wrapping SurrealDB
3. **Integration**: Connect canvas rendering to graph data
