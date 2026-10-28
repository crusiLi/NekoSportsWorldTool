//! Amap geometry is pinned once; preview and generation use the same path.
use super::{
    custom::{CustomRing, CustomRouteSpec},
    free::RoadLoop,
    generate_road::RoadPlan,
    model::Track,
};
use crate::api::model::TrackColorMode;

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub spec: CustomRouteSpec,
    pub ring: CustomRing,
}
pub struct Plan {
    path: RoadLoop,
    pub distance: f64,
    pub duration: i64,
}
impl Snapshot {
    pub fn plan(&self, dist: f64, dur: i64, seed: u64) -> Result<Plan, String> {
        if !self.spec.is_valid() || self.spec.laps < 0 || self.spec.loop_mode != self.ring.loop_mode
        {
            return Err("高德路线与当前设置不一致，请重新获取".into());
        }
        if !dist.is_finite() || dist <= 0. || dur <= 0 {
            return Err("距离或时长无效".into());
        }
        let path = RoadLoop::from_polyline(&self.ring.points_bd, self.spec.loop_mode, dist, seed)?;
        let distance = if self.spec.loop_mode {
            if self.spec.laps > 0 {
                self.spec.laps as f64 * path.length()
            } else {
                dist
            }
        } else {
            dist.min(path.length())
        };
        let duration = (distance / dist * dur as f64).round() as i64;
        let path =
            RoadLoop::from_polyline(&self.ring.points_bd, self.spec.loop_mode, distance, seed)?;
        // Validate the same supported limits before enabling the submission button.
        if !(100. ..=100_000.).contains(&distance) || !(30..=86_400).contains(&duration) {
            return Err("实际路线距离或时长超出自由跑支持范围，请调整距离或圈数".into());
        }
        Ok(Plan {
            path,
            distance,
            duration,
        })
    }
}
impl Plan {
    pub fn preview(&self) -> RoadPlan {
        self.path.preview(self.distance)
    }
    pub fn build(&self, seed: u64, start_ms: i64, color: TrackColorMode) -> Result<Track, String> {
        self.path
            .build(self.distance, self.duration, seed, start_ms, color)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot(closed: bool) -> Snapshot {
        let mut points = vec![(39.9, 116.4), (39.9, 116.402), (39.902, 116.402)];
        if closed {
            points.extend([(39.902, 116.4), (39.9, 116.4)]);
        }
        Snapshot {
            spec: CustomRouteSpec {
                origin: "fixture-a".into(),
                destination: "fixture-b".into(),
                loop_mode: closed,
                laps: 0,
            },
            ring: CustomRing {
                points_bd: points,
                leg_length_m: 9999.,
                total_length_m: 19998.,
                loop_mode: closed,
            },
        }
    }
    #[test]
    fn one_way_stops_at_actual_destination_and_keeps_corners() {
        let s = snapshot(false);
        let plan = s.plan(2000., 800, 42).unwrap();
        let preview = plan.preview();
        let track = plan
            .build(42, 1_700_000_000_000, TrackColorMode::FullGreen)
            .unwrap();
        let end = s.ring.points_bd.last().unwrap();
        let last = track.locations.last().unwrap();
        assert!((last.gLat - end.0).abs() < 1e-7 && (last.gLng - end.1).abs() < 1e-7);
        assert_eq!(preview.route.last(), Some(end));
        assert!(preview.checkpoints.is_empty());
        assert!(plan.distance < 500.);
        assert_eq!(track.totalTime, plan.duration);
        for pair in track.locations.windows(2) {
            assert!(
                (pair[0].gLat - pair[1].gLat).abs() < 1e-7
                    || (pair[0].gLng - pair[1].gLng).abs() < 1e-7,
                "cut an L-shaped corner"
            );
        }
    }
    #[test]
    fn explicit_laps_use_geometry_and_auto_laps_keep_partial_end() {
        let mut s = snapshot(true);
        s.spec.laps = 2;
        let plan = s.plan(2000., 800, 42).unwrap();
        assert!(plan.distance < 2000.);
        let p = plan.preview();
        assert_eq!(p.route.first(), p.route.last());
        let track = plan
            .build(42, 1_700_000_000_000, TrackColorMode::HalfGreenGray)
            .unwrap();
        assert!((track.totalDistance - plan.distance).abs() < 0.01);
        assert_eq!(
            track
                .stepsPerTenSec
                .iter()
                .map(|w| w.value as i64)
                .sum::<i64>(),
            track.totalSteps
        );
        s.spec.laps = 0;
        let plan = s.plan(1000., 403, 42).unwrap();
        assert_eq!(plan.distance, 1000.);
        let p = plan.preview();
        assert_ne!(p.route.first(), p.route.last());
        let t = plan
            .build(42, 1_700_000_000_000, TrackColorMode::FullGreen)
            .unwrap();
        assert!((t.totalDistance - 1000.).abs() < 0.01);
    }
    #[test]
    fn rejects_unclosed_or_invalid_snapshot() {
        let mut s = snapshot(false);
        s.spec.loop_mode = true;
        s.ring.loop_mode = true;
        assert!(s.plan(2000., 800, 42).is_err());
        s = snapshot(true);
        s.ring.points_bd[1].0 = f64::NAN;
        assert!(s.plan(2000., 800, 42).is_err());
    }
}
