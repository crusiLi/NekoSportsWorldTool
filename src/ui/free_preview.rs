//! Debounced background planning. Only the latest input key can become ready.
use crate::{location::Coordinate, track::free::RoadLoop};
use std::{
    sync::{
        mpsc::{self, Receiver},
        Arc,
    },
    time::{Duration, Instant},
};
type Reply = (String, Result<Arc<RoadLoop>, String>);
#[derive(Default)]
pub struct State {
    key: String,
    changed: Option<Instant>,
    rx: Option<Receiver<Reply>>,
    pub road: Option<Arc<RoadLoop>>,
    pub status: String,
}
impl State {
    pub fn invalidate(&mut self) {
        self.key.clear();
        self.road = None;
        self.changed = None;
    }
    pub fn ready(&self) -> bool {
        self.road.is_some()
    }
    pub fn update(
        &mut self,
        net: Arc<route_planner::RoadGraph>,
        anchor: Coordinate,
        dist: f64,
        seed: u64,
        ctx: &egui::Context,
    ) -> bool {
        let key = format!("{:p}:{:?}:{dist}:{seed}", Arc::as_ptr(&net), anchor);
        if self.key != key {
            self.key = key.clone();
            self.changed = Some(Instant::now());
            self.road = None;
            self.status = "等待规划当前方案…".into();
        }
        let mut updated = false;
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok((result_key, result)) => {
                    self.rx = None;
                    if self.key == result_key {
                        match result {
                            Ok(road) => {
                                self.status = format!(
                                    "单圈 {:.0} 米 · {:.2} 圈；终点按实际距离停留",
                                    road.length(),
                                    dist / road.length()
                                );
                                self.road = Some(road);
                            }
                            Err(e) => {
                                self.status = format!("路线规划失败：{e}");
                            }
                        }
                        updated = true;
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.rx = None;
                    self.status = "路线规划线程已结束，请重新规划".into();
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if self.rx.is_none()
            && self
                .changed
                .is_some_and(|t| t.elapsed() >= Duration::from_millis(300))
        {
            self.changed = None;
            let (tx, rx) = mpsc::channel();
            self.rx = Some(rx);
            self.status = "正在沿道路规划…".into();
            let ctx = ctx.clone();
            std::thread::spawn(move || {
                let result = crate::track::free::plan_road(&net, anchor, dist, seed).map(Arc::new);
                let _ = tx.send((key, result));
                ctx.request_repaint();
            });
        }
        if self.rx.is_some() || self.changed.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        updated
    }
}
