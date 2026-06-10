//! Window-rule matching (spec §6).
//!
//! Pure logic with no Windows dependency so it can be unit-tested anywhere.
//! Match priority: AUMID > image path > process name > window class > title regex.

use rec_core::domain::WindowRule;

/// A snapshot of one top-level window, produced by `window_detect` on Windows.
#[derive(Debug, Clone, Default)]
pub struct WindowInfo {
    pub hwnd: i64,
    pub pid: u32,
    pub title: String,
    pub class: String,
    pub image_path: Option<String>,
    pub process_name: Option<String>,
    pub aumid: Option<String>,
    pub monitor_index: Option<i32>,
    pub visible: bool,
}

/// Priority of the strongest signal that matched, higher = better (spec §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchTier {
    Title = 1,
    Class = 2,
    ProcessName = 3,
    ImagePath = 4,
    Aumid = 5,
}

fn eq_ci(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Score a single window against a rule. `None` if nothing matched.
pub fn match_tier(rule: &WindowRule, win: &WindowInfo) -> Option<MatchTier> {
    if let (Some(rule_aumid), Some(win_aumid)) = (&rule.app_user_model_id, &win.aumid) {
        if !rule_aumid.is_empty() && eq_ci(rule_aumid, win_aumid) {
            return Some(MatchTier::Aumid);
        }
    }
    if let (Some(rule_path), Some(win_path)) = (&rule.exe_path, &win.image_path) {
        if !rule_path.is_empty() && eq_ci(rule_path, win_path) {
            return Some(MatchTier::ImagePath);
        }
    }
    if let (Some(rule_proc), Some(win_proc)) = (&rule.process_name, &win.process_name) {
        if !rule_proc.is_empty() && eq_ci(rule_proc, win_proc) {
            return Some(MatchTier::ProcessName);
        }
    }
    if let Some(rule_class) = &rule.window_class {
        if !rule_class.is_empty() && eq_ci(rule_class, &win.class) {
            return Some(MatchTier::Class);
        }
    }
    if let Some(pattern) = &rule.window_title_pattern {
        if !pattern.is_empty() {
            if let Ok(re) = regex::Regex::new(pattern) {
                if re.is_match(&win.title) {
                    return Some(MatchTier::Title);
                }
            }
        }
    }
    None
}

/// Pick the best-matching visible window for a rule.
///
/// Ranks by match tier, then breaks ties with `preferred_monitor_index`, then a
/// previously-seen `last_hwnd`, then window order (spec §6).
pub fn pick_best<'a>(rule: &WindowRule, windows: &'a [WindowInfo]) -> Option<&'a WindowInfo> {
    windows
        .iter()
        .filter(|w| w.visible)
        .filter_map(|w| match_tier(rule, w).map(|t| (t, w)))
        .max_by(|(ta, wa), (tb, wb)| {
            ta.cmp(tb)
                .then_with(|| {
                    monitor_pref(rule, wa).cmp(&monitor_pref(rule, wb))
                })
                .then_with(|| last_hwnd_pref(rule, wa).cmp(&last_hwnd_pref(rule, wb)))
        })
        .map(|(_, w)| w)
}

fn monitor_pref(rule: &WindowRule, w: &WindowInfo) -> u8 {
    match (rule.preferred_monitor_index, w.monitor_index) {
        (Some(p), Some(m)) if p == m => 1,
        _ => 0,
    }
}

fn last_hwnd_pref(rule: &WindowRule, w: &WindowInfo) -> u8 {
    match rule.last_hwnd {
        Some(h) if h == w.hwnd => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(hwnd: i64, title: &str, class: &str, proc: &str) -> WindowInfo {
        WindowInfo {
            hwnd,
            pid: 100 + hwnd as u32,
            title: title.into(),
            class: class.into(),
            image_path: Some(format!("C:\\Games\\{proc}")),
            process_name: Some(proc.into()),
            aumid: None,
            monitor_index: Some(0),
            visible: true,
        }
    }

    #[test]
    fn aumid_beats_everything() {
        let rule = WindowRule {
            app_user_model_id: Some("Microsoft.Forza_8wekyb".into()),
            process_name: Some("ForzaHorizon5.exe".into()),
            ..Default::default()
        };
        let mut game = win(1, "Forza", "UnrealWindow", "ForzaHorizon5.exe");
        game.aumid = Some("Microsoft.Forza_8wekyb".into());
        let other = win(2, "Forza", "UnrealWindow", "ForzaHorizon5.exe");
        let wins = vec![other, game];
        let best = pick_best(&rule, &wins).unwrap();
        assert_eq!(best.hwnd, 1);
        assert_eq!(match_tier(&rule, best), Some(MatchTier::Aumid));
    }

    #[test]
    fn process_name_match() {
        let rule = WindowRule {
            process_name: Some("forzahorizon5.exe".into()), // case-insensitive
            ..Default::default()
        };
        let wins = vec![win(1, "x", "y", "ForzaHorizon5.exe")];
        assert_eq!(pick_best(&rule, &wins).unwrap().hwnd, 1);
    }

    #[test]
    fn title_regex_lowest_priority() {
        let rule = WindowRule {
            window_title_pattern: Some(r"^Forza".into()),
            ..Default::default()
        };
        let wins = [win(1, "Forza Horizon 5", "C", "p.exe")];
        assert_eq!(match_tier(&rule, &wins[0]), Some(MatchTier::Title));
    }

    #[test]
    fn invisible_windows_ignored() {
        let rule = WindowRule {
            process_name: Some("p.exe".into()),
            ..Default::default()
        };
        let mut w = win(1, "x", "y", "p.exe");
        w.visible = false;
        assert!(pick_best(&rule, &[w]).is_none());
    }

    #[test]
    fn no_match_returns_none() {
        let rule = WindowRule {
            process_name: Some("other.exe".into()),
            ..Default::default()
        };
        let wins = vec![win(1, "x", "y", "p.exe")];
        assert!(pick_best(&rule, &wins).is_none());
    }

    #[test]
    fn tie_broken_by_last_hwnd() {
        let rule = WindowRule {
            process_name: Some("p.exe".into()),
            last_hwnd: Some(2),
            ..Default::default()
        };
        let wins = vec![win(1, "a", "c", "p.exe"), win(2, "b", "c", "p.exe")];
        assert_eq!(pick_best(&rule, &wins).unwrap().hwnd, 2);
    }
}
