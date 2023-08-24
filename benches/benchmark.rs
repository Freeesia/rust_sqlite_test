use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use futures::future::join_all;
use rusqlite::{params, Connection, OpenFlags, Result};
use tokio::task;

const MEMORY_DB_URI: &str = "file::memory:?cache=shared";

fn setup_db() -> Result<Connection> {
    let conn = Connection::open_with_flags(
        MEMORY_DB_URI,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_URI,
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS table1 (id INTEGER PRIMARY KEY, data TEXT NOT NULL)",
        params![],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS table2 (id INTEGER PRIMARY KEY, data TEXT NOT NULL)",
        params![],
    )?;

    for i in 0..10000 {
        conn.execute(
            "INSERT INTO table1 (data) VALUES (?1)",
            params![format!("data{}", i)],
        )?;
        conn.execute(
            "INSERT INTO table2 (data) VALUES (?1)",
            params![format!("data{}", i)],
        )?;
    }

    Ok(conn)
}

async fn fetch_data_from_table(flags: OpenFlags, table_name: String) -> Result<Vec<String>> {
    // let current_thread = std::thread::current();
    // let thread_id = current_thread.id();
    // println!("Current thread ID: {:?}", thread_id);
    let conn = Connection::open_with_flags(MEMORY_DB_URI, flags)?;

    let mut stmt = conn.prepare(&format!("SELECT data FROM {}", table_name))?;
    let rows = stmt.query_map(params![], |row| row.get(0))?;
    let mut data = Vec::new();
    for row in rows {
        data.push(row?);
    }
    Ok(data)
}

fn parallel_fetch_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("SQLite Mutex Benchmark");

    let conn = setup_db().expect("Failed to setup database");
    let default_frag = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_URI;

    for &use_nomutex in &[false, true] {
        let flags = if use_nomutex {
            default_frag | OpenFlags::SQLITE_OPEN_NO_MUTEX
        } else {
            default_frag
        };

        group.bench_with_input(
            BenchmarkId::from_parameter(use_nomutex),
            &flags,
            |b, &flags| {
                b.iter(|| {
                    let runtime = tokio::runtime::Runtime::new().unwrap();
                    runtime.block_on(async {
                        let tasks: Vec<_> = (0..10)
                            .map(|_| {
                                task::spawn(async move {
                                    fetch_data_from_table(flags, "table1".to_string())
                                        .await
                                        .expect("Failed to fetch data")
                                })
                            })
                            .collect();

                        let results: Vec<_> = join_all(tasks).await;
                        for result in results {
                            result.expect("Task panicked");
                        }
                    });
                })
            },
        );
    }

    group.finish();
    conn.close().expect("明示的閉じる")
}

fn performance_config() -> Criterion {
    Criterion::default()
    // .sample_size(10)
}
criterion_group! {
    name = benches;
    config = performance_config();
    targets = parallel_fetch_benchmark
}
criterion_main!(benches);
