//! Media state poller: playerctl every 2s, sends only on change.

use std::sync::mpsc::Sender;
use std::time::Duration;

use crate::layout::MediaState;

pub fn spawn(tx: Sender<MediaState>) {
    std::thread::spawn(move || {
        let mut last: Option<MediaState> = None;
        loop {
            let state = probe();
            if last != Some(state) {
                last = Some(state);
                let _ = tx.send(state);
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

fn probe() -> MediaState {
    let out = std::process::Command::new("playerctl")
        .args(["status", "--format", "{{status}}"])
        .env("LC_ALL", "C")
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
