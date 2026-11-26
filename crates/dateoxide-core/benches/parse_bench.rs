use criterion::{black_box, criterion_group, criterion_main, Criterion};
use dateoxide_core::{parse_batch, parse_with_settings, search::search_dates, settings::Settings};

fn bench_dateoxide_core(c: &mut Criterion) {
    let settings = Settings::default();
    let dashes = "-".repeat(5000);
    let batch_inputs: Vec<&str> = [
        "2026-11-26T15:30:00Z",
        "12 December 2025",
        "3 gün önce",
        "vor 2 Wochen",
        "2026-W48-4",
        "not a date",
    ]
    .into_iter()
    .cycle()
    .take(1000)
    .collect();

    c.bench_function("parse_iso_fast_path", |b| {
        b.iter(|| parse_with_settings(black_box("2026-11-26T15:30:00Z"), &settings))
    });

    c.bench_function("parse_multilingual_relative", |b| {
        b.iter(|| parse_with_settings(black_box("3 gün önce"), &settings))
    });

    c.bench_function("parse_multilingual_absolute", |b| {
        b.iter(|| parse_with_settings(black_box("12 December 2025"), &settings))
    });

    c.bench_function("search_dates_mixed_sentence", |b| {
        b.iter(|| {
            search_dates(
                black_box("Berlin: 12. Dezember 2025, Paris: hier à 14h30"),
                &settings,
            )
        })
    });

    c.bench_function("redos_dashes_5000", |b| {
        b.iter(|| parse_with_settings(black_box(&dashes), &settings))
    });

    c.bench_function("parse_batch_1000", |b| {
        b.iter(|| parse_batch(black_box(&batch_inputs), &settings))
    });
}

criterion_group!(benches, bench_dateoxide_core);
criterion_main!(benches);
