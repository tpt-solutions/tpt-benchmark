//! Measurement layers: probes that record one metric over a batch of iterations.

/// One batch's measurement, as reported by a [`MeasurementLayer`].
pub trait SampleData: Copy + Send + Sync + 'static {
    /// The metric's value for the whole batch (e.g. total nanoseconds).
    ///
    /// The orchestrator divides it by the batch's iteration count to get the
    /// per-iteration value that the statistics are computed over.
    fn value(&self) -> f64;
}

/// A probe that measures a specific metric during the benchmark loop.
///
/// Layers are independent: each benchmark run activates one layer, and the
/// framework re-runs the benchmark for every additional layer.
pub trait MeasurementLayer: Send + Sync {
    /// The per-batch sample this layer produces.
    type Sample: SampleData;

    /// Short human-readable name, e.g. `"WallClock"`.
    fn name(&self) -> &'static str;

    /// Called once immediately before a batch of iterations begins.
    fn start_batch(&mut self);

    /// Called immediately after the batch finishes.
    fn end_batch(&mut self) -> Self::Sample;
}

/// Object-safe view of a [`MeasurementLayer`], with the sample reduced to its value.
pub(crate) trait ErasedLayer {
    fn name(&self) -> &'static str;
    fn start_batch(&mut self);
    fn end_batch(&mut self) -> f64;
}

impl<L: MeasurementLayer> ErasedLayer for L {
    fn name(&self) -> &'static str {
        MeasurementLayer::name(self)
    }

    fn start_batch(&mut self) {
        MeasurementLayer::start_batch(self);
    }

    fn end_batch(&mut self) -> f64 {
        MeasurementLayer::end_batch(self).value()
    }
}
