//! Deterministic USD-to-millionth conversions, independent of provider token budgets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidAmount;
fn convert(usd: f64, reservation: bool) -> Result<i64, InvalidAmount> {
    if !usd.is_finite() || usd < 0.0 {
        return Err(InvalidAmount);
    }
    let scaled = usd * 1_000_000.0;
    let value = if reservation {
        scaled.ceil()
    } else {
        scaled.round()
    };
    // i64::MAX rounds up when represented as f64: reject that boundary as well.
    if value >= i64::MAX as f64 {
        return Err(InvalidAmount);
    }
    Ok(value as i64)
}
pub fn reported_micros(usd: f64) -> Result<i64, InvalidAmount> {
    convert(usd, false)
}
pub fn reservation_micros(usd: f64) -> Result<i64, InvalidAmount> {
    convert(usd, true)
}
