//! Free-run geometry: plan a reusable road loop, then stop at the requested
//! distance. Every crossed road vertex is retained in the final samples.
use super::{
    generate_road::{align_network, RoadPlan},
    geom::round_to,
    model::Track,
};
use crate::{api::model::TrackColorMode, location::Coordinate};
use route_planner::{Coord, RoadGraph, Route, RouteOptions, RoutePoint};

pub struct RoadLoop {
    closed: bool,
    route: Route,
    anchor: Coordinate,
    seed: u64,
    base_target: f64,
    edges: Vec<Vec<(f64, f64)>>,
    buildings: Vec<Vec<(f64, f64)>>,
}

fn dimensions(anchor: Coordinate, dist: f64, dur: i64) -> Result<(), String> {
    anchor.validate()?;
    if (anchor.latitude == 0. && anchor.longitude == 0.) || anchor.latitude.abs() > 80. {
        return Err("请先设置有效的自由跑位置".into());
    }
    if !dist.is_finite() || !(100. ..=100_000.).contains(&dist) || !(30..=86_400).contains(&dur) {
        return Err("自由跑距离或时长超出支持范围".into());
    }
    Ok(())
}

pub fn plan_road(
    net: &RoadGraph,
    anchor: Coordinate,
    dist: f64,
    seed: u64,
) -> Result<RoadLoop, String> {
    dimensions(anchor, dist, 600)?;
    let mut g = net.clone();
    align_network(&mut g);
    // 0.4 auto-stitches disconnected components. Those synthetic connections
    // are not OSM roads and must not be used in strict free-run road mode.
    g.graph.retain_edges(|graph, e| graph[e].way_id != 0);
    g.rebuild_index();
    let start_coord = Coord::new(anchor.longitude, anchor.latitude);
    let gap = g
        .nearest_edge(g.to_m(start_coord))
        .ok_or("路网为空或当前位置附近没有道路")?
        .3;
    if gap > 150. {
        return Err(format!(
            "定位距最近道路 {gap:.0} 米，超过 150 米；请检查位置和路网范围"
        ));
    }
    let start = route_planner::project::add_virtual_nodes(&mut g, &[start_coord])?[0];
    let base_target = dist.min(600.);
    let path = route_planner::route::plan_loop(
        &g,
        start,
        &[],
        &[],
        base_target,
        seed,
        &RouteOptions {
            min_radius_m: 0.,
            ..Default::default()
        },
    )?;
    if path.len() < 3
        || path.last() != Some(&start)
        || path
            .windows(2)
            .any(|w| g.graph.find_edge(w[0], w[1]).is_none())
    {
        return Err("无法沿已有道路闭合，请换位置或扩大路网范围".into());
    }
    let mut points = Vec::new();
    let mut length = 0.;
    for coord in route_planner::route::path_coords(&g, &path) {
        if let Some(p) = points.last() {
            let p: &RoutePoint = p;
            let delta = route_planner::graph::dist(g.to_m(Coord::new(p.lon, p.lat)), g.to_m(coord));
            if delta < 1e-6 {
                continue;
            }
            length += delta;
        }
        points.push(RoutePoint {
            lat: coord.lat,
            lon: coord.lon,
            s: length,
            curvature: 0.,
        });
    }
    if length < 30. || !length.is_finite() {
        return Err("道路闭环过短，无法生成路线".into());
    }
    Ok(RoadLoop {
        closed: true,
        route: Route {
            points,
            length_m: length,
        },
        anchor,
        seed,
        base_target,
        edges: g
            .edges_deg()
            .iter()
            .map(|e| e.iter().map(|p| (p.lat, p.lon)).collect())
            .collect(),
        buildings: g
            .buildings
            .iter()
            .map(|e| e.iter().map(|p| (p.lat, p.lon)).collect())
            .collect(),
    })
}

impl RoadLoop {
    /// Geometry already returned by a routing provider, in BD-09 coordinates.
    pub fn from_polyline(
        coords: &[(f64, f64)],
        closed: bool,
        dist: f64,
        seed: u64,
    ) -> Result<Self, String> {
        let &(lat, lon) = coords.first().ok_or("高德路线为空")?;
        let anchor = Coordinate::new(lat, lon, 0.)?;
        let meter_lon = 111_320. * lat.to_radians().cos();
        let mut points: Vec<RoutePoint> = Vec::new();
        let mut length = 0.;
        for &(lat, lon) in coords {
            Coordinate::new(lat, lon, 0.)?;
            if let Some(p) = points.last() {
                let delta = ((lat - p.lat) * route_planner::graph::MET_PER_DEG_LAT)
                    .hypot((lon - p.lon) * meter_lon);
                if delta < 1e-6 {
                    continue;
                }
                length += delta;
            }
            points.push(RoutePoint {
                lat,
                lon,
                s: length,
                curvature: 0.,
            });
        }
        if points.len() < 2 || length < 30. || !length.is_finite() {
            return Err("高德路线过短或无效".into());
        }
        if closed {
            let last = points.last().unwrap();
            let gap = ((last.lat - lat) * route_planner::graph::MET_PER_DEG_LAT)
                .hypot((last.lon - lon) * meter_lon);
            if gap > 2. {
                return Err("高德往返路线未闭合，请重新获取路线".into());
            }
            if gap > 1e-6 {
                length += gap;
                points.push(RoutePoint {
                    lat,
                    lon,
                    s: length,
                    curvature: 0.,
                });
            }
        }
        Ok(Self {
            closed,
            route: Route {
                points,
                length_m: length,
            },
            anchor,
            seed,
            base_target: dist.min(600.),
            edges: vec![],
            buildings: vec![],
        })
    }
    pub fn matches(&self, anchor: Coordinate, dist: f64, seed: u64) -> bool {
        self.anchor == anchor && self.seed == seed && self.base_target == dist.min(600.)
    }
    pub fn length(&self) -> f64 {
        self.route.length_m
    }
    fn point(&self, distance: f64) -> (f64, f64, f64) {
        let s = if self.closed {
            distance.rem_euclid(self.length())
        } else {
            distance.clamp(0., self.length())
        };
        let i = self
            .route
            .points
            .partition_point(|p| p.s <= s)
            .clamp(1, self.route.points.len() - 1);
        let (a, b) = (&self.route.points[i - 1], &self.route.points[i]);
        let k = (s - a.s) / (b.s - a.s);
        let lat = a.lat + (b.lat - a.lat) * k;
        let lon = a.lon + (b.lon - a.lon) * k;
        let heading = ((b.lon - a.lon) * lat.to_radians().cos())
            .atan2(b.lat - a.lat)
            .to_degrees()
            .rem_euclid(360.);
        (lat, lon, heading)
    }
    fn vertices(&self, dist: f64) -> Vec<f64> {
        let mut out = vec![0.];
        let mut lap = 0.;
        while lap < dist {
            for p in self.route.points.iter().skip(1) {
                let s = lap + p.s;
                if s < dist - 1e-7 {
                    out.push(s);
                }
            }
            lap += self.length();
            if !self.closed {
                break;
            }
        }
        out.push(dist);
        out
    }
    pub fn preview(&self, dist: f64) -> RoadPlan {
        RoadPlan {
            route: self
                .vertices(dist)
                .iter()
                .map(|s| {
                    let (a, b, _) = self.point(*s);
                    (a, b)
                })
                .collect(),
            length_m: dist,
            edges: self.edges.clone(),
            buildings: self.buildings.clone(),
            checkpoints: vec![],
            fences: vec![],
        }
    }
    pub fn build(
        &self,
        dist: f64,
        dur: i64,
        seed: u64,
        start_ms: i64,
        color: TrackColorMode,
    ) -> Result<Track, String> {
        dimensions(self.anchor, dist, dur)?;
        if !self.closed && dist > self.length() + 0.01 {
            return Err("单程距离超过路线全长，请重新预览".into());
        }
        if !self.matches(self.anchor, dist, seed) {
            return Err("路线预览已过期，请重新规划".into());
        }
        let mut track = build_with_color(dist, dur, seed, start_ms, self.anchor, color)?;
        let mut samples = std::mem::take(&mut track.locations);
        // The classic generator may emit an initial nonzero counter. Include
        // an explicit zero sample so window sums and geometric length agree.
        let mut zero = samples[0].clone();
        zero.totalDis = 0.;
        zero.validDis = 0.;
        zero.totalTime = 0;
        zero.validTime = 0;
        zero.steps = 0;
        zero.gainTimeMs = start_ms;
        zero.gainTime = super::geom::fmt_gain_time(start_ms);
        if samples[0].totalDis > 0. {
            samples.insert(0, zero);
        }
        let vertices = self.vertices(dist);
        let mut next = 1;
        let mut result = Vec::with_capacity(samples.len() + vertices.len());
        for pair in samples.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if result.is_empty() {
                result.push(a.clone());
            }
            while next < vertices.len() && vertices[next] < b.totalDis - 1e-7 {
                let s = vertices[next];
                next += 1;
                if s <= a.totalDis + 1e-7 {
                    continue;
                }
                let k = (s - a.totalDis) / (b.totalDis - a.totalDis);
                let mut p = a.clone();
                p.totalDis = s;
                p.totalTime = a.totalTime + ((b.totalTime - a.totalTime) as f64 * k).round() as i64;
                p.validDis = a.validDis + (b.validDis - a.validDis) * k;
                p.validTime = a.validTime + ((b.validTime - a.validTime) as f64 * k).round() as i64;
                p.steps = a.steps + ((b.steps - a.steps) as f64 * k).round() as i64;
                p.bdA = a.bdA + (b.bdA - a.bdA) * k;
                p.speed = a.speed + (b.speed - a.speed) * k;
                p.gainTimeMs =
                    a.gainTimeMs + ((b.gainTimeMs - a.gainTimeMs) as f64 * k).round() as i64;
                p.gainTime = super::geom::fmt_gain_time(p.gainTimeMs);
                result.push(p);
            }
            result.push(b.clone());
        }
        for (i, p) in result.iter_mut().enumerate() {
            let (lat, lon, heading) = self.point(p.totalDis);
            p.gLat = round_to(lat, 7);
            p.gLng = round_to(lon, 7);
            p.bdD = round_to(heading, 2);
            p.queueNum = i as i64;
            p.id = 50_000 + i as i64;
        }
        track.locations = result;
        track.startLatitude = track.locations[0].gLat;
        track.startLongitude = track.locations[0].gLng;
        super::stats::rebuild(&mut track);
        track.validate_consistency()?;
        Ok(track)
    }
}

pub fn build_with_color(
    dist: f64,
    dur: i64,
    seed: u64,
    start_ms: i64,
    anchor: Coordinate,
    color: TrackColorMode,
) -> Result<Track, String> {
    dimensions(anchor, dist, dur)?;
    let radius = (dist / std::f64::consts::TAU).clamp(40., 180.);
    let lon_scale = 111_320. * anchor.latitude.to_radians().cos();
    let points: Vec<_> = (0..8)
        .map(|i| {
            let a = i as f64 * std::f64::consts::TAU / 8.;
            (
                anchor.latitude + radius * a.sin() / 111_320.,
                anchor.longitude + radius * a.cos() / lon_scale,
            )
        })
        .collect();
    let mut track = super::generator::build_with_color_mode(
        dist,
        dur,
        seed,
        (anchor.latitude, anchor.longitude),
        start_ms,
        &points,
        color,
    );
    if track.locations[0].totalDis > 0. {
        let mut zero = track.locations[0].clone();
        zero.totalDis = 0.;
        zero.validDis = 0.;
        zero.totalTime = 0;
        zero.validTime = 0;
        zero.steps = 0;
        zero.gainTimeMs = start_ms;
        zero.gainTime = super::geom::fmt_gain_time(start_ms);
        zero.id = 0;
        zero.queueNum = 0;
        track.locations.insert(0, zero);
    }
    super::stats::rebuild(&mut track);
    track.validate_consistency()?;
    Ok(track)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (RoadGraph, Coordinate) {
        let xml=br#"<osm><node id="1" lat="39.9" lon="116.4"/><node id="2" lat="39.9" lon="116.401"/><node id="3" lat="39.901" lon="116.401"/><node id="4" lat="39.901" lon="116.4"/><way id="20"><nd ref="1"/><nd ref="2"/><nd ref="3"/><nd ref="4"/><nd ref="1"/><tag k="highway" v="footway"/></way></osm>"#;
        let net = route_planner::load_osm(xml).unwrap();
        let (lat, lon) = super::super::geom::wgs84_to_bd09(39.9, 116.4);
        (net, Coordinate::new(lat, lon, 0.).unwrap())
    }
    #[test]
    fn repeated_and_partial_laps_keep_every_corner_and_target() {
        let (raw, anchor) = fixture();
        let road = plan_road(&raw, anchor, 2000., 42).unwrap();
        assert!(road.matches(anchor, 5000., 42));
        let mut aligned = raw.clone();
        align_network(&mut aligned);
        for dist in [2000., 5000., road.length() * 2.5] {
            let track = road
                .build(dist, 803, 42, 1_700_000_000_000, TrackColorMode::FullGreen)
                .unwrap();
            assert!((track.totalDistance - dist).abs() < 0.01);
            let preview = road.preview(dist);
            let last = track.locations.last().unwrap();
            let end = preview.route.last().unwrap();
            assert!((last.gLat - end.0).abs() < 1e-7 && (last.gLng - end.1).abs() < 1e-7);
            for pair in track.locations.windows(2) {
                // Checking interior points catches diagonals across road corners.
                for k in 0..=10 {
                    let t = k as f64 / 10.;
                    let q = Coord::new(
                        pair[0].gLng + (pair[1].gLng - pair[0].gLng) * t,
                        pair[0].gLat + (pair[1].gLat - pair[0].gLat) * t,
                    );
                    assert!(
                        aligned.nearest_edge(aligned.to_m(q)).unwrap().3 < 0.1,
                        "segment cut corner"
                    );
                }
            }
            assert!(
                (track.speedPerTenSec.iter().map(|w| w.value).sum::<f64>() - dist).abs() < 0.01
            );
            assert_eq!(
                track
                    .stepsPerTenSec
                    .iter()
                    .map(|w| w.value as i64)
                    .sum::<i64>(),
                track.totalSteps
            );
            assert_eq!(
                track.speedPerTenSec.iter().map(|w| w.time).sum::<i64>(),
                803
            );
        }
    }
    #[test]
    fn reject_missing_distant_or_unreturnable_roads() {
        let (raw, anchor) = fixture();
        assert!(plan_road(&RoadGraph::new(), anchor, 2000., 42).is_err());
        let far = Coordinate::new(anchor.latitude + 0.1, anchor.longitude, 0.).unwrap();
        assert!(plan_road(&raw, far, 2000., 42).is_err());
        let mut directed = raw.clone();
        directed.graph.retain_edges(|g, e| {
            g.edge_endpoints(e).unwrap().0.index() < g.edge_endpoints(e).unwrap().1.index()
        });
        directed.rebuild_index();
        assert!(plan_road(&directed, anchor, 2000., 42).is_err());
        assert!(plan_road(&raw, anchor, f64::NAN, 42).is_err());
    }

    #[test]
    fn exact_laps_close_but_partial_laps_do_not_teleport_home() {
        let (raw, anchor) = fixture();
        let road = plan_road(&raw, anchor, 2000., 91).unwrap();
        for (laps, closed) in [(2., true), (2.5, false)] {
            let dist = road.length() * laps;
            let t = road
                .build(dist, 803, 91, 1_700_000_000_000, TrackColorMode::FullGreen)
                .unwrap();
            let first = t.locations.first().unwrap();
            let last = t.locations.last().unwrap();
            let gap = (first.gLat - last.gLat).abs() + (first.gLng - last.gLng).abs();
            assert_eq!(gap < 1e-6, closed);
            assert!(road.preview(dist).checkpoints.is_empty());
        }
    }
}
