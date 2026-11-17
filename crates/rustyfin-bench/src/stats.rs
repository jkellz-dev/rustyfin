//! Percentiles over request times.

/// The `p`th percentile (nearest rank) of `values`; 0 when there are none.
pub fn percentile(values: &[f64], p: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let rank = ((p / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

#[cfg(test)]
mod tests {
    use super::percentile;

    #[test]
    fn nearest_rank() {
        let v: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(percentile(&v, 50.0), 50.0);
        assert_eq!(percentile(&v, 95.0), 95.0);
        assert_eq!(percentile(&v, 100.0), 100.0);
        assert_eq!(percentile(&v, 0.0), 1.0);
    }

    #[test]
    fn order_does_not_matter() {
        assert_eq!(percentile(&[3.0, 1.0, 2.0], 50.0), 2.0);
    }

    #[test]
    fn empty_is_zero() {
        assert_eq!(percentile(&[], 95.0), 0.0);
    }
}
