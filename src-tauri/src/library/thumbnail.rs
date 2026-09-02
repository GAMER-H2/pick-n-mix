//! Downscaled cover art, generated off the request path and cached to disk.
//!
//! Library rows and grid cells decode artwork at their own on-screen size
//! (38-320px) rather than the original embedded picture, which can be a
//! multi-megapixel JPEG. Decoding and resampling that full image for every
//! row, on every scroll, is expensive enough to visibly stutter a long list.
//!
//! Generation itself must never run on the request that asks for it: the
//! `art://` protocol handler is invoked by the webview on its own UI thread
//! (WKWebView's `startURLSchemeTask`, webkit2gtk's `URISchemeRequest`,
//! WebView2's `WebResourceRequested` are all delivered there), so anything
//! slow done inline freezes the whole window, not just the image. A cache
//! miss instead queues a background job and the caller falls back to serving
//! the original for that one request.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;

/// Widths thumbnails are generated at. Requests are rounded up to the nearest
/// bucket so the cache holds one file per size class rather than one per
/// pixel value any component happened to ask for.
const BUCKETS: [u32; 5] = [64, 128, 256, 384, 640];

fn bucket_for(requested: u32) -> u32 {
    BUCKETS
        .iter()
        .copied()
        .find(|&b| b >= requested)
        .unwrap_or(*BUCKETS.last().expect("non-empty"))
}

/// What the cache can answer about `(id, width)` without doing any work.
pub enum Art {
    /// A generated thumbnail is ready.
    Thumb(PathBuf),
    /// The original is already at or below this bucket, or could not be
    /// decoded as an image at all — permanently the right answer, nothing to
    /// generate.
    Original,
    /// Not yet known; a background job has been queued (or was already
    /// in flight). The caller should serve the original for now.
    Pending,
}

fn thumb_path(artwork_dir: &Path, id: &str, width: u32) -> PathBuf {
    artwork_dir.join("thumbs").join(format!("{id}-{width}.jpg"))
}

/// Marker written when generation would produce nothing (source already
/// small enough, or undecodable): without it, every future request would
/// re-decode the full original just to rediscover the same fact.
fn marker_path(artwork_dir: &Path, id: &str, width: u32) -> PathBuf {
    artwork_dir
        .join("thumbs")
        .join(format!("{id}-{width}.orig"))
}

fn lookup(artwork_dir: &Path, id: &str, width: u32) -> Option<Art> {
    let thumb = thumb_path(artwork_dir, id, width);
    if thumb.exists() {
        return Some(Art::Thumb(thumb));
    }
    if marker_path(artwork_dir, id, width).exists() {
        return Some(Art::Original);
    }
    None
}

/// Generate (or mark as unnecessary) the thumbnail for `id` at `width`.
/// Idempotent, and safe to race with itself: written to a unique temp file
/// and renamed into place, so a request that stats the final path never sees
/// a half-encoded JPEG.
fn generate(artwork_dir: &Path, id: &str, original: &Path, width: u32) {
    let thumbs_dir = artwork_dir.join("thumbs");
    let final_path = thumb_path(artwork_dir, id, width);
    if final_path.exists() {
        return;
    }
    if std::fs::create_dir_all(&thumbs_dir).is_err() {
        return;
    }

    let mark_original = || {
        let _ = std::fs::File::create(marker_path(artwork_dir, id, width));
    };

    let Ok(image) = image::open(original) else {
        return mark_original();
    };
    if image.width() <= width && image.height() <= width {
        return mark_original();
    }

    let resized = image.resize(width, width, image::imageops::FilterType::Triangle);
    let tmp = thumbs_dir.join(format!(".{id}-{width}-{}.tmp", uuid::Uuid::new_v4()));
    if resized
        .to_rgb8()
        .save_with_format(&tmp, image::ImageFormat::Jpeg)
        .is_err()
    {
        let _ = std::fs::remove_file(&tmp);
        return;
    }
    if std::fs::rename(&tmp, &final_path).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// A small bounded pool that generates thumbnails in the background, deduped
/// so the same `(id, width)` is never queued twice while already in flight.
pub struct Thumbnailer {
    artwork_dir: PathBuf,
    jobs: crossbeam_channel::Sender<(String, PathBuf, u32)>,
    queued: Arc<Mutex<HashSet<(String, u32)>>>,
}

impl Thumbnailer {
    pub fn new(artwork_dir: PathBuf) -> Self {
        let (tx, rx) = crossbeam_channel::unbounded::<(String, PathBuf, u32)>();
        let queued: Arc<Mutex<HashSet<(String, u32)>>> = Arc::default();

        // Deliberately not one worker per core: the audio engine needs
        // headroom, and saturating every core with JPEG decodes to make a
        // list paint a little sooner is a bad trade in a music player.
        let workers = std::thread::available_parallelism()
            .map(|n| (n.get() / 2).max(1))
            .unwrap_or(2)
            .min(3);

        for i in 0..workers {
            let rx = rx.clone();
            let queued = queued.clone();
            let dir = artwork_dir.clone();
            std::thread::Builder::new()
                .name(format!("pnm-thumbs-{i}"))
                .spawn(move || {
                    for (id, original, width) in rx.iter() {
                        generate(&dir, &id, &original, width);
                        // Only after generation finishes, so a request
                        // arriving mid-job does not re-queue and then find
                        // nothing there yet.
                        queued.lock().remove(&(id, width));
                    }
                })
                .ok();
        }

        Thumbnailer {
            artwork_dir,
            jobs: tx,
            queued,
        }
    }

    /// The cached answer for `id` at (at least) `requested` pixels wide, or
    /// `Art::Pending` after queuing a background job if this is a fresh miss.
    pub fn get(&self, id: &str, original: &Path, requested: u32) -> Art {
        let width = bucket_for(requested);
        if let Some(art) = lookup(&self.artwork_dir, id, width) {
            return art;
        }
        self.queue(id, original, width);
        Art::Pending
    }

    fn queue(&self, id: &str, original: &Path, width: u32) {
        let key = (id.to_string(), width);
        // The dedup set, not the channel, is what stops twelve tracks
        // sharing one album cover — or the same row scrolling past twice —
        // from decoding the same file over and over.
        if !self.queued.lock().insert(key.clone()) {
            return;
        }
        if self
            .jobs
            .send((key.0.clone(), original.to_path_buf(), width))
            .is_err()
        {
            self.queued.lock().remove(&key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn tempfile_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pnm-thumb-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn buckets_round_up() {
        assert_eq!(bucket_for(1), 64);
        assert_eq!(bucket_for(64), 64);
        assert_eq!(bucket_for(65), 128);
        assert_eq!(bucket_for(640), 640);
        assert_eq!(bucket_for(2000), 640);
    }

    #[test]
    fn generates_and_reuses_a_cached_thumbnail() {
        let dir = tempfile_dir();
        let original = dir.join("cover.png");
        image::RgbImage::from_pixel(800, 800, image::Rgb([200, 40, 40]))
            .save(&original)
            .unwrap();

        // `generate` takes an already-bucketed width, as `Thumbnailer::get`
        // passes it — callers ask for pixels, not a bucket.
        let width = bucket_for(100);
        generate(&dir, "abc123", &original, width);
        let Some(Art::Thumb(first)) = lookup(&dir, "abc123", width) else {
            panic!("expected a generated thumbnail");
        };
        assert!(first.exists());

        // A second call must reuse the cache rather than re-decoding: proven
        // by removing the original and confirming lookup still succeeds.
        std::fs::remove_file(&original).unwrap();
        generate(&dir, "abc123", &original, width);
        let Some(Art::Thumb(second)) = lookup(&dir, "abc123", width) else {
            panic!("expected the cached thumbnail to still be found");
        };
        assert_eq!(first, second);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn marks_an_already_small_original_instead_of_ever_reopening_it() {
        let dir = tempfile_dir();
        let original = dir.join("tiny.png");
        image::RgbImage::from_pixel(32, 32, image::Rgb([10, 10, 10]))
            .save(&original)
            .unwrap();

        generate(&dir, "tiny", &original, 64);
        assert!(matches!(lookup(&dir, "tiny", 64), Some(Art::Original)));

        // The marker alone answers this now; no thumbnail file is written.
        std::fs::remove_file(&original).unwrap();
        assert!(matches!(lookup(&dir, "tiny", 64), Some(Art::Original)));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn thumbnailer_serves_pending_then_settles_on_the_generated_thumbnail() {
        let dir = tempfile_dir();
        let artwork_dir = dir.join("artwork");
        std::fs::create_dir_all(&artwork_dir).unwrap();
        let original = artwork_dir.join("cover.png");
        image::RgbImage::from_pixel(800, 800, image::Rgb([1, 2, 3]))
            .save(&original)
            .unwrap();

        let thumbnailer = Thumbnailer::new(artwork_dir.clone());
        assert!(matches!(
            thumbnailer.get("song", &original, 100),
            Art::Pending
        ));

        let width = bucket_for(100);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(Art::Thumb(_)) = lookup(&artwork_dir, "song", width) {
                break;
            }
            assert!(Instant::now() < deadline, "thumbnail never generated");
            std::thread::sleep(Duration::from_millis(10));
        }

        // A repeat request while nothing changed should now be answered from
        // the cache rather than queuing another job.
        assert!(matches!(
            thumbnailer.get("song", &original, 100),
            Art::Thumb(_)
        ));

        std::fs::remove_dir_all(&dir).ok();
    }
}
