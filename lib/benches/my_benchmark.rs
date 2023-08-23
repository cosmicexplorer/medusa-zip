/*
 * Description: ???
 *
 * Copyright (C) 2023 Danny McClanahan <dmcC2@hypnicjerk.ai>
 * SPDX-License-Identifier: Apache-2.0
 *
 * Licensed under the Apache License, Version 2.0 (see LICENSE).
 */

use criterion::{black_box, criterion_group, criterion_main, Criterion};

#[inline]
fn fibonacci(n: u64) -> u64 {
  match n {
    0 => 1,
    1 => 1,
    n => fibonacci(n - 1) + fibonacci(n - 2),
  }
}

pub fn criterion_benchmark(c: &mut Criterion) {
  c.bench_function("fib 20", |b| b.iter(|| fibonacci(black_box(20))));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
