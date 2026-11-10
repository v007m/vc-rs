use std::time::Duration;

use super::{HEADER, Latencies, read_varint, unzigzag, write_varint, zigzag};
use crate::LoadError;

fn latencies(micros: &[u64]) -> Latencies {
    let mut latencies = Latencies::new().unwrap();
    for &value in micros {
        latencies.record(Duration::from_micros(value));
    }
    latencies
}

/// Every non-empty bucket as (highest equivalent value, count).
fn buckets(latencies: &Latencies) -> Vec<(u64, u64)> {
    latencies.histogram.iter_recorded().map(|b| (b.value_iterated_to(), b.count_at_value())).collect()
}

fn header(payload: u32) -> Vec<u8> {
    let mut bytes = vec![0x1c, 0x84, 0x93, 0x13];
    bytes.extend_from_slice(&payload.to_be_bytes());
    bytes.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 3]);
    bytes.extend_from_slice(&1_u64.to_be_bytes());
    bytes.extend_from_slice(&60_000_000_u64.to_be_bytes());
    bytes.extend_from_slice(&1.0_f64.to_be_bytes());
    bytes
}

#[test]
fn the_encoding_is_hdrhistograms_uncompressed_v2_layout() {
    assert_eq!(latencies(&[]).encode(), [header(1), vec![0x00]].concat());
    assert_eq!(latencies(&[1]).encode(), [header(2), vec![0x00, 0x02]].concat());
    assert_eq!(latencies(&[5, 5, 6]).encode(), [header(3), vec![0x09, 0x04, 0x02]].concat());
}

#[test]
fn decoding_an_encoding_gives_back_the_recorded_buckets() {
    let mut micros: Vec<u64> = (0..2_000).map(|i| (i * 7_919) % 250_000).collect();
    micros.extend([0, 1, 2_047, 2_048, 1_000_000, 59_999_999, 60_000_000, 90_000_000]);
    let recorded = latencies(&micros);
    let decoded = Latencies::decode(&recorded.encode()).unwrap();
    assert_eq!(buckets(&decoded), buckets(&recorded));
    assert_eq!(decoded.count(), recorded.count());
    for quantile in [0.5, 0.9, 0.95, 0.99, 1.0] {
        assert!((decoded.quantile_ms(quantile) - recorded.quantile_ms(quantile)).abs() < f64::EPSILON);
    }
    assert!((decoded.max_ms() - recorded.max_ms()).abs() < f64::EPSILON);
    assert_eq!(decoded.count_at(1_000_000), 1);
}

#[test]
fn quantiles_and_max_read_in_milliseconds() {
    let recorded = latencies(&[1_000, 2_000, 3_000, 4_000, 120_000_000]);
    assert!((recorded.quantile_ms(0.5) - 3.0).abs() < 0.01);
    assert!((recorded.max_ms() - 60_000.0).abs() < 60.0, "a latency above 60 s counts as 60 s");
    assert!(latencies(&[]).max_ms().abs() < f64::EPSILON);
    assert!(latencies(&[]).quantile_ms(0.99).abs() < f64::EPSILON);
}

#[test]
fn merging_adds_every_latency() {
    let mut first = latencies(&[10, 20]);
    first.merge(&latencies(&[20, 30_000])).unwrap();
    assert_eq!(buckets(&first), buckets(&latencies(&[10, 20, 20, 30_000])));
}

#[test]
fn varints_follow_hdrhistogram_with_a_whole_ninth_byte() {
    for (word, bytes) in [
        (0, vec![0x00]),
        (0x7F, vec![0x7F]),
        (0x80, vec![0x80, 0x01]),
        (u64::MAX, [vec![0xFF; 8], vec![0xFF]].concat()),
        (1 << 56, [vec![0x80; 8], vec![0x01]].concat()),
    ] {
        let mut written = Vec::new();
        write_varint(&mut written, word);
        assert_eq!(written, bytes, "{word:#x}");
        assert_eq!(read_varint(&mut bytes.into_iter()), Some(word));
    }
    for value in [0, 1, -1, 63, -64, i64::MAX, i64::MIN] {
        assert_eq!(unzigzag(zigzag(value)), value);
    }
    assert_eq!((zigzag(0), zigzag(-1), zigzag(1), zigzag(-5)), (0, 1, 2, 9));
}

#[test]
fn bytes_that_are_not_a_load_histogram_are_refused() {
    let good = latencies(&[5, 70]).encode();
    let mut cookie = good.clone();
    cookie[3] = 0x14;
    let mut bounds = good.clone();
    bounds[15] = 2;
    let mut long = good.clone();
    long.push(0x02);
    let cut = [header(1), vec![0x80]].concat();
    let past = [header(3), vec![0x81, 0x80, 0x04]].concat();
    for bytes in [&good[..HEADER - 1], &cookie[..], &bounds[..], &long[..], &cut[..], &past[..]] {
        assert!(matches!(Latencies::decode(bytes), Err(LoadError::BadEncoding(_))), "{bytes:x?}");
    }
}
