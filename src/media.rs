//! Media state poller: playerctl every 2s, sends only on change.

use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::layout::MediaState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NowPlaying {
    pub state: MediaState,
    pub track: String, // "artist — title" (длинная тире), пусто если нет плеера
}

pub fn spawn(tx: Sender<NowPlaying>) {
    std::thread::spawn(move || {
        let mut last: Option<NowPlaying> = None;
        loop {
            let np = probe_full();
            if last.as_ref() != Some(&np) {
                last = Some(np.clone());
                let _ = tx.send(np);
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

fn playerctl(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("playerctl")
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// plasma-browser-integration отдаёт настоящие artist/title (браузерный MPRIS — заголовок вкладки);
/// предпочитаем его, иначе дефолтный игрок playerctl.
fn player_args() -> Vec<&'static str> {
    let list = playerctl(&["-l"]).unwrap_or_default();
    if list.lines().any(|l| l.trim() == "plasma-browser-integration") {
        vec!["-p", "plasma-browser-integration"]
    } else {
        vec![]
    }
}

fn probe_full() -> NowPlaying {
    let pre = player_args();
    let mut status_args = pre.clone();
    status_args.extend_from_slice(&["status", "--format", "{{status}}"]);
    let state = match playerctl(&status_args).as_deref() {
        Some("Playing") => MediaState::Playing,
        Some("Paused") => MediaState::Paused,
        _ => MediaState::NoPlayer,
    };
    if state == MediaState::NoPlayer {
        return NowPlaying { state, track: String::new() };
    }
    let mut meta_args = pre;
    meta_args.extend_from_slice(&["metadata", "--format", "{{artist}} — {{title}}"]);
    let mut track = playerctl(&meta_args).unwrap_or_default();
    // пустой artist даёт ведущее " — " — режем
    while track.starts_with('—') || track.starts_with(" — ") {
        track = track.trim_start_matches(['—', ' ']).to_string();
    }
    NowPlaying { state, track }
}

fn probe() -> MediaState {
    let out = std::process::Command::new("playerctl")
        .args(["status", "--format", "{{status}}"])
        .output();
    match out {
        Ok(o) if o.status.success() => match String::from_utf8_lossy(&o.stdout).trim() {
            "Playing" => MediaState::Playing,
            "Paused" => MediaState::Paused,
            _ => MediaState::NoPlayer,
        },
        _ => MediaState::NoPlayer,
    }
}

/// Fire-and-forget player command (no crash if no player: playerctl just errors).
pub fn player_cmd(what: &str) {
    let _ = std::process::Command::new("playerctl")
        .arg(what)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

pub fn volume(up: bool) {
    let _ = std::process::Command::new("wpctl")
        .args([
            "set-volume",
            "@DEFAULT_AUDIO_SINK@",
            if up { "5%+" } else { "5%-" },
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}
