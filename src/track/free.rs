//! A local loop around the configured BD-09 anchor, with no school checkpoints.
//! These shaping points are geometry only: they never become fixed_point_json.

use crate::location::Coordinate;
use super::model::Track;

pub fn build(dist: f64, dur: i64, seed: u64, start_ms: i64, anchor: Coordinate) -> Result<Track, String> {
    anchor.validate()?;
    if (anchor.latitude == 0.0 && anchor.longitude == 0.0) || anchor.latitude.abs() > 80.0 {
        return Err("请设置有效的自由跑位置（不支持零坐标或极地区域）".into());
    }
    if !dist.is_finite() || !(100.0..=100_000.0).contains(&dist) || !(30..=86_400).contains(&dur) {
        return Err("自由跑距离或时长超出支持范围".into());
    }
    // Keep the loop local; the existing arc-length sampler handles multiple laps.
    let radius = (dist / std::f64::consts::TAU).clamp(40.0, 180.0);
    let lon_scale = 111_320.0 * anchor.latitude.to_radians().cos();
    let points: Vec<_> = (0..8).map(|i| {
        let angle = i as f64 * std::f64::consts::TAU / 8.0;
        (anchor.latitude + radius * angle.sin() / 111_320.0,
         anchor.longitude + radius * angle.cos() / lon_scale)
    }).collect();
    let track = super::generator::build(dist, dur, seed,
        (anchor.latitude, anchor.longitude), start_ms, &points);
    track.validate_consistency()?;
    Ok(track)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_track_needs_no_school_points_and_stays_near_selected_location() {
        let anchor = Coordinate::new(39.9, 116.4, 0.0).unwrap();
        let track = build(2000.0, 800, 42, 1_700_000_000_000, anchor).unwrap();
        assert!(!track.locations.is_empty());
        assert!((track.totalDistance - 2000.0).abs() < 1.0);
        assert_eq!(track.totalTime, 800);
        assert!(track.locations.iter().all(|p|
            (p.gLat - anchor.latitude).abs() < 0.02 &&
            (p.gLng - anchor.longitude).abs() < 0.02));
        let again = build(2000.0, 800, 42, 1_700_000_000_000, anchor).unwrap();
        assert_eq!(track.locations[0].gLat, again.locations[0].gLat);
        assert_eq!(track.locations[0].gLng, again.locations[0].gLng);
    }

    #[test]
    fn rejects_missing_location_and_invalid_dimensions() {
        let anchor = Coordinate::new(39.9, 116.4, 0.0).unwrap();
        assert!(build(2000.0, 800, 1, 0, Coordinate::new(0.0, 0.0, 0.0).unwrap()).is_err());
        assert!(build(f64::NAN, 800, 1, 0, anchor).is_err());
        assert!(build(2000.0, 0, 1, 0, anchor).is_err());
    }
}
