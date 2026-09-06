use crate::native::Window;
use std::collections::{HashMap, VecDeque};

#[derive(Default)]
pub struct Policy {
    windows: HashMap<String, Record>,
    attempts: VecDeque<u64>,
    pub halted: bool,
}
struct Record {
    rect: [i32; 4],
    since: u64,
    completed: bool,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Block {
    Paused,
    Handled,
    RateLimit(u64),
    Cooldown(u64),
    Unstable(u64),
    Discovering,
}
impl Block {
    pub fn wait_ms(&self) -> Option<u64> {
        match self {
            Self::RateLimit(ms) | Self::Cooldown(ms) | Self::Unstable(ms) => Some(*ms),
            _ => None,
        }
    }
    pub fn message(&self) -> &'static str {
        match self {
            Self::Paused => "自动修复已暂停 · 请手动修复并检查结果",
            Self::Handled => "正在守护 · 此浮窗本轮已尝试；仍无法拖动时请手动修复",
            Self::RateLimit(_) => "暂时不能修复 · 最近五分钟已尝试三次",
            Self::Cooldown(_) => "暂时不能修复 · 请等待修复间隔结束",
            Self::Unstable(_) => "等待浮窗稳定 · 请暂时不要移动宠物",
            Self::Discovering => "正在识别浮窗 · 请稍候",
        }
    }
}
impl Policy {
    pub fn observe(&mut self, windows: &[Window], now: u64) {
        self.windows
            .retain(|key, _| windows.iter().any(|w| w.key() == *key));
        for w in windows {
            let r = self.windows.entry(w.key()).or_insert(Record {
                rect: w.rect,
                since: now,
                completed: false,
            });
            if r.rect != w.rect {
                r.rect = w.rect;
                r.since = now;
            }
        }
    }
    pub fn forget_hwnd(&mut self, hwnd: usize) {
        self.windows
            .retain(|key, _| !key.ends_with(&format!(":{hwnd}")));
    }
    pub fn allow(&mut self, w: &Window, now: u64, manual: bool) -> Result<(), Block> {
        while self
            .attempts
            .front()
            .is_some_and(|n| now.saturating_sub(*n) >= 300_000)
        {
            self.attempts.pop_front();
        }
        if !manual && self.halted {
            return Err(Block::Paused);
        }
        if !manual && self.windows.get(&w.key()).is_some_and(|r| r.completed) {
            return Err(Block::Handled);
        }
        if self.attempts.len() >= 3 {
            return Err(Block::RateLimit(
                300_000 - now.saturating_sub(*self.attempts.front().unwrap()),
            ));
        }
        if self
            .attempts
            .back()
            .is_some_and(|n| now.saturating_sub(*n) < if manual { 10_000 } else { 30_000 })
        {
            return Err(Block::Cooldown(
                (if manual { 10_000 } else { 30_000 })
                    - now.saturating_sub(*self.attempts.back().unwrap()),
            ));
        }
        let Some(r) = self.windows.get(&w.key()) else {
            return Err(Block::Discovering);
        };
        if now.saturating_sub(r.since) < 2000 {
            return Err(Block::Unstable(2000 - now.saturating_sub(r.since)));
        }
        Ok(())
    }
    pub fn attempted(&mut self, now: u64) {
        self.attempts.push_back(now);
    }
    pub fn manual_ready(&self, w: &Window, now: u64) -> bool {
        self.windows
            .get(&w.key())
            .is_some_and(|r| now.saturating_sub(r.since) >= 2000)
            && self
                .attempts
                .iter()
                .filter(|n| now.saturating_sub(**n) < 300_000)
                .count()
                < 3
            && !self
                .attempts
                .back()
                .is_some_and(|n| now.saturating_sub(*n) < 10_000)
    }
    pub fn complete(&mut self, w: &Window) {
        if let Some(r) = self.windows.get_mut(&w.key()) {
            r.completed = true;
        }
        self.halted = false;
    }
}

pub fn restore_value(original: u32, current: u32) -> (u32, bool) {
    let bit = 0x80000;
    (
        (current & !bit) | (original & bit),
        (original & !bit) != (current & !bit),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    fn w() -> Window {
        Window {
            hwnd: 5,
            style: 0x800a8,
            rect: [0, 0, 100, 100],
            owner: crate::native::Owner {
                pid: 1,
                start: 1,
                version: "test".into(),
                path: "test".into(),
                test: true,
            },
        }
    }
    #[test]
    fn stable_and_once() {
        let mut p = Policy::default();
        let mut w = w();
        p.observe(std::slice::from_ref(&w), 0);
        assert!(p.allow(&w, 1999, false).is_err());
        assert!(p.allow(&w, 2000, false).is_ok());
        p.complete(&w);
        w.rect[0] = 5;
        p.observe(std::slice::from_ref(&w), 3000);
        assert!(p.allow(&w, 100000, false).is_err());
        assert!(p.allow(&w, 100000, true).is_ok());
    }
    #[test]
    fn reopen() {
        let mut p = Policy::default();
        let w = w();
        p.observe(std::slice::from_ref(&w), 0);
        p.complete(&w);
        p.observe(&[], 3000);
        p.observe(std::slice::from_ref(&w), 4000);
        assert!(p.allow(&w, 6000, false).is_ok());
    }
    #[test]
    fn identity_reuse() {
        let mut p = Policy::default();
        let mut w = w();
        p.observe(std::slice::from_ref(&w), 0);
        p.complete(&w);
        w.owner.start = 2;
        p.observe(std::slice::from_ref(&w), 4000);
        assert!(p.allow(&w, 6000, false).is_ok());
    }
    #[test]
    fn rate_limit() {
        let mut p = Policy::default();
        let w = w();
        p.observe(std::slice::from_ref(&w), 0);
        p.attempted(2000);
        assert!(p.allow(&w, 3000, true).is_err());
        assert!(p.allow(&w, 12000, true).is_ok());
        assert!(p.allow(&w, 12000, false).is_err());
        p.attempted(32000);
        p.attempted(62000);
        assert!(p.allow(&w, 100000, true).is_err());
        assert!(p.allow(&w, 302001, true).is_ok());
    }
    #[test]
    fn conflict_keeps_app_changes() {
        assert_eq!(restore_value(0x800a8, 0xa8), (0x800a8, false));
        assert_eq!(restore_value(0x800a8, 0x88), (0x80088, true));
    }
    #[test]
    fn halted_manual_allowed() {
        let mut p = Policy::default();
        let w = w();
        p.observe(std::slice::from_ref(&w), 0);
        p.halted = true;
        assert!(p.allow(&w, 2000, false).is_err());
        assert!(p.allow(&w, 2000, true).is_ok());
    }
    #[test]
    fn thousand_lifecycles() {
        let mut p = Policy::default();
        for i in 0..1000 {
            let mut w = w();
            w.hwnd = i;
            p.observe(std::slice::from_ref(&w), i as u64 * 4000);
            assert!(p.allow(&w, i as u64 * 4000 + 2000, false).is_ok());
            p.complete(&w);
            p.forget_hwnd(w.hwnd);
        }
        assert!(p.windows.is_empty());
    }
    #[test]
    fn handled_and_paused_are_not_fake_cooldowns() {
        let mut p = Policy::default();
        let w = w();
        p.observe(std::slice::from_ref(&w), 0);
        p.attempted(2000);
        p.complete(&w);
        assert_eq!(p.allow(&w, 2500, false), Err(Block::Handled));
        assert_eq!(p.allow(&w, 2500, true), Err(Block::Cooldown(9500)));
        assert!(!p.manual_ready(&w, 2500));
        p.halted = true;
        assert_eq!(p.allow(&w, 2500, false), Err(Block::Paused));
        assert_eq!(p.allow(&w, 12000, true), Ok(()));
        assert!(p.manual_ready(&w, 12000));
    }
}
