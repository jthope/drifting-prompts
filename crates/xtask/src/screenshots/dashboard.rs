//! What IP Camera's dashboard shows in the screenshots: two weeks of made-up events from the four
//! cameras, each with its snapshot and its video, and a few snapshots taken by hand.
//!
//! Each camera keeps habits of its own: people and parcels at the front door by day, cars in the
//! driveway as people leave and come home, animals in the backyard around dawn and dusk, and plain
//! motion at the side gate by night, so the busiest hours and the week's heatmap have a shape. The
//! dashboard counts by the local clock, so the hours are placed with this machine's offset from
//! UTC, which Windows gives. The same seed gives the same events, placed back from the time of
//! the run, and none later than it.
//!
//! The files are only what the pages read: names, sizes and times. A snapshot is its camera's
//! picture, hard-linked rather than copied, with its thumbnail beside it. A video is an empty file
//! of the size a recording that long would have, sparse so it takes no room on the drive, last
//! written when it ended, which is where its length comes from; it has its camera's thumbnail too.

use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde_json::{Value, json};

/// How many days back the events go: the dashboard's 7 and 14 days.
const DAYS: i64 = 14;

/// A recording's bytes a second: a 1080p main stream at about 3.5 Mbit/s.
const BYTES_A_SECOND: u64 = 440_000;

/// The seed the events come from.
const SEED: u64 = 0x5eed_cafe_f00d_d00d;

/// What a camera tends to see, and when.
struct Habit {
    /// How many events it sees a day, at least and at most.
    per_day: (u64, u64),
    /// The local hours they start in; an hour twice is twice as likely.
    hours: &'static [i64],
    /// What its AI tells apart in an event, one of these at a time, each in the order the app
    /// lists detections; none for a camera that knows only motion.
    saw: &'static [&'static [&'static str]],
}

/// Each camera's habits, in the order of the screenshots' cameras.
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

/// Writes the events' files and a few snapshots taken by hand into `snapshots` and `videos`, from
/// the cameras' pictures in `pictures` (`<name>.png`, and `thumbnails/<name>.png`), and returns
/// `events.json`. `cameras` are each camera's id and name, in [`HABITS`]' order.
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
                // Kept to the past: a whole event, ended a second or more before now.
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

        // A snapshot or two someone took by day.
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

/// The camera `name`'s picture as the snapshot `file` in `snapshots`, with its thumbnail.
fn save_snapshot(pictures: &Path, name: &str, snapshots: &Path, file: &str) -> Result<()> {
    link(&pictures.join(format!("{name}.png")), &snapshots.join(file))?;
    link(
        &pictures.join("thumbnails").join(format!("{name}.png")),
        &snapshots.join("thumbnails").join(file),
    )
}

/// The video `file` in `videos`, `seconds` long from `start`, with the camera `name`'s thumbnail.
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
    // Sparse, its size takes no room. Without it, it would still be quick to make: Windows
    // doesn't write the zeros, though it sets the room aside.
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

/// `to` as another name of `from`, or a copy where a link can't be made.
fn link(from: &Path, to: &Path) -> Result<()> {
    fs::hard_link(from, to)
        .or_else(|_| fs::copy(from, to).map(|_| ()))
        .with_context(|| format!("couldn't write {}", to.display()))
}

/// This machine's offset from UTC now, in seconds, as the system gives it; 0 where it can't be read.
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

/// A small generator of numbers that look random, the same for the same seed (xorshift64*).
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    /// From 0 up to `count`, not including it.
    fn below(&mut self, count: u64) -> u64 {
        self.next() % count.max(1)
    }

    /// From `low` to `high`, both included.
    fn between(&mut self, low: u64, high: u64) -> u64 {
        low + self.below(high - low + 1)
    }

    /// One of `items`.
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
        // The side gate knows only motion, and its files say so.
        assert!(events.iter().any(|event| {
            event["camera"] == "cam3"
                && event["snapshot"].as_str().unwrap().ends_with(".motion.png")
        }));
    }
}
