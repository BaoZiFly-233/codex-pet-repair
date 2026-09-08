use crate::native::{Window, WindowKey};
use std::collections::{HashMap, VecDeque};

#[derive(Default)]
pub struct Policy {
    windows: HashMap<WindowKey, Record>,
    attempts: VecDeque<u64>,
    pub halted: bool,
}
struct Record {
    rect: [i32; 4],
    since: u64,
    completed: bool,
    retry: bool,
    health: InputHealth,
    evidence_at: u64,
    misses: u8,
    first_miss: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InputHealth {
    #[default]
    Unknown,
    Healthy,
    Mismatch,
    Failed,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Block {
    Paused,
    Handled,
    RateLimit(u64),
    Cooldown(u64),
    Unstable(u64),
    Discovering,
    CheckingInput,
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
            Self::CheckingInput => "正在检查输入 · 请将鼠标移到宠物上；仍无法拖动时可手动修复",
        }
    }
}
impl Policy {
    pub fn input_failed(&self, w: &Window) -> bool {
        self.windows
            .get(&w.key())
            .is_some_and(|r| r.health == InputHealth::Failed)
    }
    // Only evidence for the current identity and geometry can authorize a retry.
    pub fn input(&mut self, w: &Window, health: InputHealth, now: u64) -> bool {
        let Some(r) = self.windows.get_mut(&w.key()) else {
            return false;
        };
        if r.rect != w.rect {
            return false;
        }
        let previous = r.health;
        if health == InputHealth::Mismatch {
            if r.misses == 0 || now.saturating_sub(r.evidence_at) > 2500 {
                r.first_miss = now;
                r.misses = 0;
            }
            r.misses = r.misses.saturating_add(1);
            r.health = if r.misses >= 3 && now.saturating_sub(r.first_miss) >= 2000 {
                InputHealth::Failed
            } else {
                InputHealth::Unknown
            };
            if r.health == InputHealth::Failed {
                r.completed = false;
                r.retry = true;
            }
        } else {
            r.misses = 0;
            r.health = health;
            if health == InputHealth::Healthy {
                r.completed = true;
            }
        }
        r.evidence_at = now;
        previous != r.health
    }
    pub fn observe(&mut self, windows: &[Window], now: u64) {
        self.windows
            .retain(|key, _| windows.iter().any(|w| w.key() == *key));
        for w in windows {
            let r = self.windows.entry(w.key()).or_insert(Record {
                rect: w.rect,
                since: now,
                completed: false,
                retry: false,
                health: InputHealth::Unknown,
                evidence_at: 0,
                misses: 0,
                first_miss: 0,
            });
            if r.rect != w.rect {
                r.rect = w.rect;
                r.since = now;
                r.health = InputHealth::Unknown;
                r.misses = 0;
            }
        }
    }
    pub fn forget_hwnd(&mut self, hwnd: usize) {
        self.windows.retain(|key, _| key.hwnd != hwnd);
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
        if !manual
            && self.windows.get(&w.key()).is_some_and(|r| {
                r.retry
                    && (r.health != InputHealth::Failed || now.saturating_sub(r.evidence_at) > 2500)
            })
        {
            return Err(Block::CheckingInput);
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
        self.windows.contains_key(&w.key()) && self.next_manual_change(w, now).is_none()
    }
    pub fn complete(&mut self, w: &Window) {
        if let Some(r) = self.windows.get_mut(&w.key()) {
            r.completed = true;
            r.health = InputHealth::Unknown;
            r.misses = 0;
        }
        self.halted = false;
    }
    pub fn next_manual_change(&self, w: &Window, now: u64) -> Option<u64> {
        let mut deadline = self.windows.get(&w.key())?.since + 2000;
        if let Some(last) = self.attempts.back() {
            deadline = deadline.max(last + 10000);
        }
        let mut recent = self
            .attempts
            .iter()
            .filter(|t| now.saturating_sub(**t) < 300000);
        if let Some(first) = recent.next() {
            if recent.count() >= 2 {
                deadline = deadline.max(first + 300000);
            }
        }
        (deadline > now).then_some(deadline)
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
    fn resize_and_silence_never_authorize_a_retry() {
        let mut p = Policy::default();
        let mut w = w();
        p.observe(std::slice::from_ref(&w), 0);
        p.complete(&w);
        w.rect[2] += 20;
        p.observe(std::slice::from_ref(&w), 3000);
        for now in [4000, 5000, 6000] {
            p.input(&w, InputHealth::Unknown, now);
        }
        assert_eq!(p.allow(&w, 40000, false), Err(Block::Handled));
        p.input(&w, InputHealth::Healthy, 41000);
        assert_eq!(p.allow(&w, 42000, false), Err(Block::Handled));
    }
    #[test]
    fn confirmed_failure_requires_continuous_current_evidence() {
        let mut p = Policy::default();
        let mut w = w();
        p.observe(std::slice::from_ref(&w), 0);
        p.attempted(2000);
        p.complete(&w);
        for now in [3000, 4000] {
            p.input(&w, InputHealth::Mismatch, now);
        }
        assert_eq!(p.allow(&w, 5000, false), Err(Block::Handled));
        p.input(&w, InputHealth::Unknown, 5000); // occlusion or stale coordinates breaks the streak
        for now in [30000, 31000, 32000] {
            p.input(&w, InputHealth::Mismatch, now);
        }
        assert_eq!(p.allow(&w, 32000, false), Ok(()));
        assert_eq!(p.allow(&w, 34501, false), Err(Block::CheckingInput));
        let old = w.clone();
        w.rect[2] += 1;
        p.observe(std::slice::from_ref(&w), 35000);
        assert!(!p.input(&old, InputHealth::Mismatch, 36000));
        assert_eq!(p.allow(&w, 38000, false), Err(Block::CheckingInput));
        p.input(&w, InputHealth::Healthy, 39000);
        assert_eq!(p.allow(&w, 40000, false), Err(Block::Handled));
    }
    #[test]
    fn failure_never_erases_limits_pause_or_identity() {
        let mut p = Policy::default();
        let w = w();
        p.observe(std::slice::from_ref(&w), 0);
        for time in [2000, 32000, 62000] {
            p.attempted(time);
        }
        p.complete(&w);
        for now in [70000, 71000, 72000] {
            p.input(&w, InputHealth::Mismatch, now);
        }
        assert_eq!(p.allow(&w, 72000, false), Err(Block::RateLimit(230000)));
        p.halted = true;
        assert_eq!(p.allow(&w, 72000, false), Err(Block::Paused));
        let mut other = w.clone();
        other.owner.start += 1;
        assert!(!p.input(&other, InputHealth::Healthy, 72001));
        p.complete(&w);
        p.halted = false;
        assert_eq!(p.allow(&w, 400000, false), Err(Block::Handled));
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
    fn manual_wakeup_tracks_real_deadlines() {
        let mut p = Policy::default();
        let w = w();
        assert!(!p.manual_ready(&w, 100));
        p.observe(std::slice::from_ref(&w), 100);
        assert_eq!(p.next_manual_change(&w, 100), Some(2100));
        assert!(!p.manual_ready(&w, 2099));
        assert!(p.manual_ready(&w, 2100));
        assert_eq!(p.next_manual_change(&w, 2100), None);
        p.attempted(2100);
        assert_eq!(p.next_manual_change(&w, 3000), Some(12100));
        assert!(!p.manual_ready(&w, 12099));
        assert!(p.manual_ready(&w, 12100));
        p.attempted(40000);
        p.attempted(80000);
        assert_eq!(p.next_manual_change(&w, 100000), Some(302100));
        assert!(!p.manual_ready(&w, 302099));
        assert!(p.manual_ready(&w, 302100));
        let mut reopened = w.clone();
        reopened.owner.start += 1;
        assert!(!p.manual_ready(&reopened, 302100));
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
