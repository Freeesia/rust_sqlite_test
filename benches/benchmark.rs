use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use futures::future::join_all;
use once_cell::sync::Lazy;
use rusqlite::{params, Connection, OpenFlags, Result};
use tokio::task;
use uuid::Uuid;

static MEMORY_DB_URI: Lazy<String> =
    Lazy::new(|| format!("/tmp/{}.sqlite3", Uuid::new_v4().to_string()));

fn setup_db() -> Result<Connection> {
    println!("Creating database at {}", *MEMORY_DB_URI);
    let mut conn = Connection::open_with_flags(&*MEMORY_DB_URI, OpenFlags::default())?;

    for t in 0..10 {
        conn.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS table{} (id INTEGER PRIMARY KEY, data TEXT NOT NULL)",
                t
            ),
            params![],
        )?;

        let tx = conn.transaction()?;
        for i in 0..10000 {
            tx.execute(
                &format!("INSERT INTO table{} (data) VALUES (?)", t),
                params![format!("{}-{}", t, i)],
            )?;
        }
        tx.commit()?;
    }

    Ok(conn)
}

async fn fetch_data_from_table(flags: OpenFlags, table_name: String) -> Result<Vec<String>> {
    // let current_thread = std::thread::current();
    // let thread_id = current_thread.id();
    // println!("Start thread ID: {:?}", thread_id);
    let conn = Connection::open_with_flags(&*MEMORY_DB_URI, flags)?;

    let mut stmt = conn.prepare(&format!("SELECT data FROM {}", table_name))?;
    let rows = stmt.query_map(params![], |row| row.get(0))?;
    let mut data = Vec::new();
    for row in rows {
        data.push(row?);
    }
    // println!("End thread ID: {:?}", thread_id);
    Ok(data)
}

fn parallel_fetch_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("SQLite Mutex Benchmark");
    let rt = tokio::runtime::Runtime::new().unwrap();

    let conn = setup_db().expect("Failed to setup database");
    let flags_with_mutex = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_URI;
    let flags_without_mutex = flags_with_mutex | OpenFlags::SQLITE_OPEN_NO_MUTEX;

    for &n in &[10, 50, 100] {
        group.bench_with_input(BenchmarkId::new("With Mutex 1", n), &n, |b, &n| {
            b.iter(|| {
                rt.block_on(async {
                    let mut handles = Vec::with_capacity(n);
                    for _ in 0..n {
                        let handle = task::spawn(fetch_data_from_table(
                            flags_with_mutex,
                            "table1".to_string(),
                        ));
                        handles.push(handle);
                    }
                    let results: Vec<_> = join_all(handles).await;
                    for result in results {
                        let _ = result.expect("Task panicked");
                    }
                })
            })
        });

        group.bench_with_input(BenchmarkId::new("Without Mutex 1", n), &n, |b, &n| {
            b.iter(|| {
                rt.block_on(async {
                    let mut handles = Vec::with_capacity(n);
                    for _ in 0..n {
                        let handle = task::spawn(fetch_data_from_table(
                            flags_without_mutex,
                            "table1".to_string(),
                        ));
                        handles.push(handle);
                    }
                    let results: Vec<_> = join_all(handles).await;
                    for result in results {
                        let _ = result.expect("Task panicked");
                    }
                })
            })
        });

        group.bench_with_input(BenchmarkId::new("With Mutex 10", n), &n, |b, &n| {
            b.iter(|| {
                rt.block_on(async {
                    let mut handles = Vec::with_capacity(n);
                    for _ in 0..n {
                        let handle = task::spawn(fetch_data_from_table(
                            flags_with_mutex,
                            format!("table{}", n % 10),
                        ));
                        handles.push(handle);
                    }
                    let results: Vec<_> = join_all(handles).await;
                    for result in results {
                        let _ = result.expect("Task panicked");
                    }
                })
            })
        });

        group.bench_with_input(BenchmarkId::new("Without Mutex 10", n), &n, |b, &n| {
            b.iter(|| {
                rt.block_on(async {
                    let mut handles = Vec::with_capacity(n);
                    for _ in 0..n {
                        let handle = task::spawn(fetch_data_from_table(
                            flags_without_mutex,
                            format!("table{}", n % 10),
                        ));
                        handles.push(handle);
                    }
                    let results: Vec<_> = join_all(handles).await;
                    for result in results {
                        let _ = result.expect("Task panicked");
                    }
                })
            })
        });
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
