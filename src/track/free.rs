//! Free runs from the configured BD-09 anchor: local loop or imported OSM roads.
//! Geometry points never become school checkpoints or fixed_point_json.

use crate::location::Coordinate;
use super::model::Track;
use super::generate_road::{align_network, RoadPlan};
use route_planner::{Coord, RoadGraph, Route, RouteOptions};

// OSM is WGS84; saved device coordinates are BD-09. Align a clone once.
fn road_route(net: &RoadGraph, anchor: Coordinate, dist: f64, seed: u64) -> Result<(RoadGraph, Route), String> {
    anchor.validate()?;
    if !dist.is_finite() || !(100.0..=100_000.0).contains(&dist) {
        return Err("自由跑目标距离无效".into());
    }
    let mut aligned = net.clone();
    align_network(&mut aligned);
    let start = Coord::new(anchor.longitude, anchor.latitude);
    let (_, _, _, gap) = aligned.nearest_edge(aligned.to_m(start))
        .ok_or("当前位置附近找不到道路，请导入覆盖跑步位置的 OSM 路网")?;
    if gap > 150.0 {
        return Err(format!("定位锚点距最近道路 {gap:.0} 米，超过 150 米；请检查位置或导入附近路网"));
    }
    // Keep corners on the road polyline rather than rounding through buildings.
    let opts = RouteOptions { min_radius_m: 0.0, ..RouteOptions::default() };
    let route = route_planner::plan_route_split(&aligned, &[start], &[], dist, seed, &opts)?;
    validate_road_loop(&aligned, &route, dist)?;
    Ok((aligned, route))
}

fn validate_road_loop(net: &RoadGraph, route: &Route, target: f64) -> Result<(), String> {
    let first = route.points.first().ok_or("路网路线为空")?;
    let last = route.points.last().ok_or("路网路线为空")?;
    let gap = route_planner::graph::dist(
        net.to_m(Coord::new(first.lon, first.lat)), net.to_m(Coord::new(last.lon, last.lat)));
    if gap > 1.0 || route.length_m < 30.0 || !route.length_m.is_finite() {
        return Err("路网无法生成闭合路线，请扩大路网范围或调整位置".into());
    }
    if (route.length_m - target).abs() > (target * 0.10).max(30.0) {
        return Err(format!("可规划路线 {:.0} 米与目标 {:.0} 米相差过大，请调整距离或扩大路网范围", route.length_m, target));
    }
    Ok(())
}

fn point_on_route(route: &Route, distance: f64) -> (f64, f64, f64) {
    let s = distance.clamp(0.0, route.length_m);
    let i = route.points.partition_point(|p| p.s < s).max(1).min(route.points.len() - 1);
    let (a, b) = (&route.points[i - 1], &route.points[i]);
    let t = if b.s > a.s { (s - a.s) / (b.s - a.s) } else { 0.0 };
    let lat = a.lat + (b.lat - a.lat) * t;
    let lon = a.lon + (b.lon - a.lon) * t;
    let heading = ((b.lon - a.lon) * lat.to_radians().cos()).atan2(b.lat - a.lat)
        .to_degrees().rem_euclid(360.0);
    (lat, lon, heading)
}

/// Preview and submission use the same aligned road geometry, with no checkpoint/fence input.
pub fn road_preview(net: &RoadGraph, anchor: Coordinate, dist: f64, seed: u64) -> Result<RoadPlan, String> {
    let (aligned, route) = road_route(net, anchor, dist, seed)?;
    Ok(RoadPlan {
        route: route.points.iter().map(|p| (p.lat, p.lon)).collect(),
        length_m: route.length_m,
        edges: aligned.edges_deg().iter().map(|e| e.iter().map(|p| (p.lat, p.lon)).collect()).collect(),
        buildings: aligned.buildings.iter().map(|e| e.iter().map(|p| (p.lat, p.lon)).collect()).collect(),
        checkpoints: Vec::new(), fences: Vec::new(),
    })
}

pub fn build_road(net: &RoadGraph, anchor: Coordinate, dist: f64, dur: i64, seed: u64, start_ms: i64) -> Result<Track, String> {
    let (_, route) = road_route(net, anchor, dist, seed)?;
    // Reuse the existing time/sensor/serialization pipeline. Report actual road length;
    // replace every coordinate (including abnormal/sentinel points) with road samples.
    let mut track = build(route.length_m, dur, seed, start_ms, anchor)?;
    for point in &mut track.locations {
        let (lat, lon, heading) = point_on_route(&route, point.totalDis);
        point.gLat = super::geom::round_to(lat, 7);
        point.gLng = super::geom::round_to(lon, 7);
        point.bdD = super::geom::round_to(heading, 2);
    }
    if let Some(last) = track.locations.last_mut() {
        let first = route.points.first().unwrap();
        last.gLat = super::geom::round_to(first.lat, 7);
        last.gLng = super::geom::round_to(first.lon, 7);
    }
    track.startLatitude = track.locations[0].gLat;
    track.startLongitude = track.locations[0].gLng;
    track.validate_consistency()?;
    Ok(track)
}

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

    fn road_fixture() -> (RoadGraph, Coordinate) {
        // Synthetic WGS84 street grid with a building inside one block.
        let mut xml = String::from("<osm version=\"0.6\">");
        for y in 0..7 {
            for x in 0..7 {
                xml.push_str(&format!("<node id=\"{}\" lat=\"{}\" lon=\"{}\"/>", y*7+x+1,
                    39.9 + y as f64 * 0.0008, 116.4 + x as f64 * 0.001));
            }
        }
        let mut way = 100;
        for y in 0..7 {
            for x in 0..7 {
                let a = y*7+x+1;
                for b in [if x<6 { Some(a+1) } else { None }, if y<6 { Some(a+7) } else { None }].into_iter().flatten() {
                    xml.push_str(&format!("<way id=\"{way}\"><nd ref=\"{a}\"/><nd ref=\"{b}\"/><tag k=\"highway\" v=\"footway\"/></way>"));
                    way += 1;
                }
            }
        }
        xml.push_str("</osm>");
        let mut net = route_planner::load_osm(xml.as_bytes()).unwrap();
        net.buildings.push(vec![Coord::new(116.4022,39.9018),Coord::new(116.4028,39.9018),
            Coord::new(116.4028,39.9022),Coord::new(116.4022,39.9022)]);
        let (lat,lon) = super::super::geom::wgs84_to_bd09(39.9024,116.403);
        (net,Coordinate::new(lat,lon,0.0).unwrap())
    }

    #[test]
    fn road_preview_and_track_follow_same_roads_without_checkpoints() {
        let (raw, anchor) = road_fixture();
        let original = raw.node_coord(raw.graph.node_indices().next().unwrap());
        for seed in [42, 91] {
            let preview = road_preview(&raw, anchor, 2000.0, seed).unwrap();
            assert!(preview.checkpoints.is_empty() && preview.fences.is_empty());
            let track = build_road(&raw, anchor, 2000.0, 800, seed, 1_700_000_000_000).unwrap();
            assert!((track.totalDistance-preview.length_m).abs()<0.001);
            assert_eq!((track.startLatitude,track.startLongitude),
                (track.locations.last().unwrap().gLat,track.locations.last().unwrap().gLng));
            let mut aligned=raw.clone(); align_network(&mut aligned);
            let route=road_route(&raw,anchor,2000.0,seed).unwrap().1;
            for point in &track.locations {
                let q=Coord::new(point.gLng,point.gLat);
                assert!(aligned.nearest_edge(aligned.to_m(q)).unwrap().3<1.0, "point left roads");
                assert!(!aligned.buildings.iter().any(|b| route_planner::point_in_polygon(b,q)));
                let (lat,lon,_)=point_on_route(&route,point.totalDis);
                assert!((point.gLat-lat).abs()<0.00000011 && (point.gLng-lon).abs()<0.00000011);
            }
        }
        assert_eq!(raw.node_coord(raw.graph.node_indices().next().unwrap()),original, "input must remain WGS84");
    }

    #[test]
    fn road_mode_rejects_missing_or_distant_network_and_non_closed_routes() {
        let (raw,anchor)=road_fixture();
        assert!(road_preview(&RoadGraph::new(),anchor,2000.0,42).is_err());
        let far=Coordinate::new(anchor.latitude+0.1,anchor.longitude,0.0).unwrap();
        assert!(road_preview(&raw,far,2000.0,42).is_err());
        let one_way=br#"<osm><node id="1" lat="39.9" lon="116.4"/><node id="2" lat="39.901" lon="116.4"/><way id="1"><nd ref="1"/><nd ref="2"/><tag k="highway" v="footway"/><tag k="oneway" v="yes"/></way></osm>"#;
        let net=route_planner::load_osm(one_way).unwrap();
        let (lat,lon)=super::super::geom::wgs84_to_bd09(39.9,116.4);
        assert!(road_preview(&net,Coordinate::new(lat,lon,0.0).unwrap(),2000.0,42).is_err());
        assert!(road_preview(&raw,anchor,f64::NAN,42).is_err());
    }

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
