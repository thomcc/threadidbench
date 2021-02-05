
use criterion::*;

criterion::criterion_group!(benches, tidbench::thread_id_benches);
criterion::criterion_main!(benches);

