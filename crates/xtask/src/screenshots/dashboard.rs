//! Made-up events for IP Camera's dashboard screenshots: two weeks from the four cameras, with
//! their files. Hours are local, as the dashboard counts them; a video is an empty sparse file of
//! a recording's size, last written when it ended.

use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde_json::{Value, json};

const DAYS: i64 = 14;

const BYTES_A_SECOND: u64 = 440_000;

const SEED: u64 = 0x5eed_cafe_f00d_d00d;

struct Habit {
    per_day: (u64, u64),
    /// Local hours they start in; an hour listed twice is twice as likely.
    hours: &'static [i64],
    /// One of these at a time, each in the order the app lists detections; none for motion only.
    saw: &'static [&'static [&'static str]],
}

/// In the screenshots' cameras' order.
const HABITS: [Habit; 4] = [
    // The front door.
    Habit {
        per_day: (2, 5),
        hours: &[8, 9, 10, 11, 11, 12, 13, 14, 15, 16, 17, 18, 18, 19],
        saw: &[
            &["person"],
            &["person"],
            &["person", "package"],
            &["package"],
        ],
    },
    // The driveway.
    Habit {
        per_day: (3, 6),
        hours: &[7, 7, 8, 8, 8, 12, 13, 17, 17, 18, 18, 19],
        saw: &[
            &["vehicle"],
            &["vehicle"],
            &["person", "vehicle"],
            &["person"],
        ],
    },
    // The backyard.
    Habit {
        per_day: (1, 4),
        hours: &[5, 6, 6, 7, 15, 16, 16, 17, 20, 21],
        saw: &[&["animal"], &["animal"], &["person"]],
    },
    // The side gate.
    Habit {
        per_day: (1, 3),
        hours: &[0, 1, 2, 3, 4, 21, 22, 23, 23],
        saw: &[],
    },
];

/// Writes the events' files and returns `events.json`; `cameras` are in [`HABITS`]' order.
pub fn seed(
    cameras: &[(String, &str)],
    pictures: &Path,
    snapshots: &Path,
    videos: &Path,
) -> Result<Value> {
    for folder in [snapshots, videos] {
        fs::create_dir_all(folder.join("thumbnails"))?;
    }
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
    let offset = local_offset();
    let today = (now + offset).div_euclid(86_400) * 86_400;
    let mut random = Random(SEED);
    let mut events = Vec::new();

    for day in (0..DAYS).rev() {
        let midnight = today - day * 86_400;
        for ((id, name), habit) in cameras.iter().zip(&HABITS) {
            for _ in 0..random.between(habit.per_day.0, habit.per_day.1) {
                let local = midnight + random.pick(habit.hours) * 3600 + random.below(3600) as i64;
                let saw: &[&str] = if habit.saw.is_empty() {
                    &[]
                } else {
                    random.pick(habit.saw)
                };
                let seconds = random.between(20, 110);
                let sightings = random.between(1, 2);
                let millisecond = random.below(1000);
                let start = local - offset;
                // Only whole events, ended before now.
                if start + seconds as i64 >= now {
                    continue;
                }
                let start = start as u64;
                let tags = if saw.is_empty() { &["motion"][..] } else { saw };
                let suffix: String = tags.iter().map(|tag| format!(".{tag}")).collect();
                let snapshot = format!("snapshot-{start}-{id}{suffix}.png");
                let video = format!("video-{start}-{id}{suffix}.mp4");

                save_snapshot(pictures, name, snapshots, &snapshot)?;
                save_video(pictures, name, videos, &video, start, seconds)?;
                let started = start * 1000 + millisecond;
                events.push(json!({
                    "id": format!("{started}-{id}"),
                    "camera": id,
                    "started": started,
                    "ended": started + seconds * 1000,
                    "motion": true,
                    "detected": saw,
                    "sightings": sightings,
                    "snapshot": snapshot,
                    "videos": [video],
                }));
            }
        }

        for _ in 0..random.below(3) {
            let (id, name) = random.pick(cameras);
            let start =
                midnight + random.between(9, 19) as i64 * 3600 + random.below(3600) as i64 - offset;
            if start < now {
                save_snapshot(
                    pictures,
                    name,
                    snapshots,
                    &format!("snapshot-{start}-{id}.png"),
                )?;
            }
        }
    }

    events.sort_by_key(|event| event["started"].as_u64());
    Ok(json!({ "version": 1, "events": events }))
}

fn save_snapshot(pictures: &Path, name: &str, snapshots: &Path, file: &str) -> Result<()> {
    link(&pictures.join(format!("{name}.png")), &snapshots.join(file))?;
    link(
        &pictures.join("thumbnails").join(format!("{name}.png")),
        &snapshots.join("thumbnails").join(file),
    )
}

fn save_video(
    pictures: &Path,
    name: &str,
    videos: &Path,
    file: &str,
    start: u64,
    seconds: u64,
) -> Result<()> {
    let path = videos.join(file);
    fs::File::create(&path).with_context(|| format!("couldn't write {}", path.display()))?;
    // Sparse, so the size takes no room; if that fails, Windows still writes no zeros.
    let _ = Command::new("fsutil")
        .args(["sparse", "setflag"])
        .arg(&path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let video = fs::OpenOptions::new().write(true).open(&path)?;
    video.set_len(seconds * BYTES_A_SECOND)?;
    video.set_modified(UNIX_EPOCH + Duration::from_secs(start + seconds))?;
    link(
        &pictures.join("thumbnails").join(format!("{name}.png")),
        &videos
            .join("thumbnails")
            .join(Path::new(file).with_extension("png")),
    )
}

fn link(from: &Path, to: &Path) -> Result<()> {
    fs::hard_link(from, to)
        .or_else(|_| fs::copy(from, to).map(|_| ()))
        .with_context(|| format!("couldn't write {}", to.display()))
}

fn local_offset() -> i64 {
    let offset = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[int][TimeZoneInfo]::Local.GetUtcOffset([DateTime]::UtcNow).TotalSeconds",
        ])
        .stderr(Stdio::null())
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|text| text.trim().parse().ok());
    offset.unwrap_or_else(|| {
        println!("cargo screenshots: couldn't read the time zone, so the events keep UTC's hours");
        0
    })
}

/// xorshift64*: the same numbers for the same seed.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn below(&mut self, count: u64) -> u64 {
        self.next() % count.max(1)
    }

    /// From `low` to `high`, both included.
    fn between(&mut self, low: u64, high: u64) -> u64 {
        low + self.below(high - low + 1)
    }

    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len() as u64) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_events_are_in_the_past_oldest_first_each_with_its_files() {
        let pictures = tempfile::tempdir().unwrap();
        let media = tempfile::tempdir().unwrap();
        fs::create_dir_all(pictures.path().join("thumbnails")).unwrap();
        let cameras: Vec<(String, &str)> = ["Front door", "Driveway", "Backyard", "Side gate"]
            .into_iter()
            .enumerate()
            .map(|(index, name)| (format!("cam{index}"), name))
            .collect();
        for (_, name) in &cameras {
            fs::write(pictures.path().join(format!("{name}.png")), "picture").unwrap();
            fs::write(
                pictures
                    .path()
                    .join("thumbnails")
                    .join(format!("{name}.png")),
                "thumbnail",
            )
            .unwrap();
        }
        let (snapshots, videos) = (media.path().join("snapshots"), media.path().join("videos"));

        let log = seed(&cameras, pictures.path(), &snapshots, &videos).unwrap();

        let events = log["events"].as_array().unwrap();
        assert!(events.len() > 100, "{} events", events.len());
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        let mut last = 0;
        for event in events {
            let started = event["started"].as_u64().unwrap();
            assert!(started >= last && event["ended"].as_u64().unwrap() <= now);
            last = started;
            let snapshot = event["snapshot"].as_str().unwrap();
            assert!(snapshots.join(snapshot).is_file());
            assert!(snapshots.join("thumbnails").join(snapshot).is_file());
            let video = videos.join(event["videos"][0].as_str().unwrap());
            let seconds = (event["ended"].as_u64().unwrap() - started) / 1000;
            assert_eq!(
                fs::metadata(&video).unwrap().len(),
                seconds * BYTES_A_SECOND
            );
        }
        assert!(events.iter().any(|event| {
            event["camera"] == "cam3"
                && event["snapshot"].as_str().unwrap().ends_with(".motion.png")
        }));
    }
}
