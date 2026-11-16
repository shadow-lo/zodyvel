//! Odak (Pomodoro) döngüsü: zamanlayıcının üstünde çalışır. 25 dk odak, 5 dk mola; dördüncü odaktan
//! sonra 15 dk uzun mola ve döngü biter. Aşama dolunca sıradaki hemen başlar (iş parçacığı yine yalnız
//! bitiş anına kadar uyur). "Odak modu" özelliği kapalıysa döngü başlamaz ve süren döngü düz zamanlayıcı
//! gibi biter. Saf: zaman ve özellik dışarıdan verilir (testli).

use serde::{Deserialize, Serialize};

pub const WORK_MIN: u32 = 25;
pub const BREAK_MIN: u32 = 5;
pub const LONG_BREAK_MIN: u32 = 15;
/// Uzun moladan önceki odak sayısı.
pub const ROUNDS: u32 = 4;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    #[default]
    Work,
    Break,
    LongBreak,
}

impl Phase {
    pub fn minutes(self) -> u32 {
        match self {
            Phase::Work => WORK_MIN,
            Phase::Break => BREAK_MIN,
            Phase::LongBreak => LONG_BREAK_MIN,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
pub struct Focus {
    pub phase: Phase,
    /// Kaçıncı odak (1..=ROUNDS); molada az önce biten odağın sırası.
    pub round: u32,
}

impl Default for Focus {
    fn default() -> Self {
        Self { phase: Phase::Work, round: 1 }
    }
}

/// Bir aşama dolunca sıradaki; None: döngü bitti (uzun mola doldu).
pub fn next(focus: Focus) -> Option<Focus> {
    let round = focus.round.clamp(1, ROUNDS);
    match focus.phase {
        Phase::Work if round >= ROUNDS => Some(Focus { phase: Phase::LongBreak, round }),
        Phase::Work => Some(Focus { phase: Phase::Break, round }),
        Phase::Break => Some(Focus { phase: Phase::Work, round: round + 1 }),
        Phase::LongBreak => None,
    }
}

/// Arayüze giden "aşama bitti" olayı (`today://focus`): çentikte kısa not olur.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FocusStep {
    pub ended: Phase,
    /// Başlayan aşama; None: döngü tamam.
    pub next: Option<Focus>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_runs_four_rounds_then_long_break_then_ends() {
        let mut f = Focus::default();
        let mut phases = vec![(f.phase, f.round)];
        while let Some(n) = next(f) {
            f = n;
            phases.push((f.phase, f.round));
        }
        use Phase::*;
        assert_eq!(
            phases,
            [(Work, 1), (Break, 1), (Work, 2), (Break, 2), (Work, 3), (Break, 3), (Work, 4), (LongBreak, 4)]
        );
        let total: u32 = phases.iter().map(|(p, _)| p.minutes()).sum();
        assert_eq!(total, 4 * 25 + 3 * 5 + 15);
    }

    #[test]
    fn odd_round_from_file_stays_in_range() {
        assert_eq!(next(Focus { phase: Phase::Work, round: 9 }), Some(Focus { phase: Phase::LongBreak, round: 4 }));
        assert_eq!(next(Focus { phase: Phase::Break, round: 0 }), Some(Focus { phase: Phase::Work, round: 2 }));
    }

    #[test]
    fn serializes_like_the_ui_expects() {
        let json = serde_json::to_string(&Focus { phase: Phase::LongBreak, round: 4 }).unwrap();
        assert_eq!(json, r#"{"phase":"longBreak","round":4}"#);
    }
}
