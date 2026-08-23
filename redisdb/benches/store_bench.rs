use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use redisops::handler::{self, AuthConfig};
use redisops::store::Store;
use std::sync::Arc;
use tokio::sync::RwLock;

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
}

async fn d(store: Store, args: Vec<&str>) {
    let args: Vec<String> = args.into_iter().map(String::from).collect();
    let _ = handler::dispatch(store, args, Arc::new(RwLock::new(AuthConfig::default()))).await;
}

fn bench_set_get(c: &mut Criterion) {
    let runtime = rt();
    let store = Store::new();

    let mut group = c.benchmark_group("kv_store");
    for size in [1, 10, 100, 1000] {
        group.bench_with_input(BenchmarkId::new("set", size), &size, |b, &size| {
            b.iter(|| {
                runtime.block_on(async {
                    for i in 0..size {
                        d(
                            store.clone(),
                            vec!["SET", &format!("key:{i}"), &format!("value:{i}")],
                        )
                        .await;
                    }
                });
            });
        });

        group.bench_with_input(BenchmarkId::new("get", size), &size, |b, &size| {
            runtime.block_on(async {
                for i in 0..size {
                    d(store.clone(), vec!["SET", &format!("key:{i}"), "value"]).await;
                }
            });
            b.iter(|| {
                runtime.block_on(async {
                    for i in 0..size {
                        d(store.clone(), vec!["GET", &format!("key:{i}")]).await;
                    }
                });
            });
        });
    }
    group.finish();
}

fn bench_list(c: &mut Criterion) {
    let runtime = rt();
    let store = Store::new();

    let mut group = c.benchmark_group("list");
    for size in [10, 100, 1000] {
        group.bench_with_input(BenchmarkId::new("lpush_rpop", size), &size, |b, &size| {
            b.iter(|| {
                runtime.block_on(async {
                    for i in 0..size {
                        d(
                            store.clone(),
                            vec!["LPUSH", "bench:list", &format!("item:{i}")],
                        )
                        .await;
                        d(store.clone(), vec!["RPOP", "bench:list"]).await;
                    }
                });
            });
        });
    }
    group.finish();
}

fn bench_concurrent(c: &mut Criterion) {
    let runtime = rt();
    let store = Store::new();

    let mut group = c.benchmark_group("concurrent");
    group.bench_function("parallel_writes_100", |b| {
        b.iter(|| {
            runtime.block_on(async {
                let mut handles = vec![];
                for i in 0..100 {
                    let s = store.clone();
                    handles.push(tokio::spawn(async move {
                        d(s, vec!["SET", &format!("conc:{i}"), "v"]).await;
                    }));
                }
                for h in handles {
                    h.await.unwrap();
                }
            });
        });
    });
    group.finish();
}

criterion_group!(benches, bench_set_get, bench_list, bench_concurrent);
criterion_main!(benches);
