//! Continuous chapter viewer driven over mpv's JSON IPC socket.
//!
//! One mpv window shows local page images (or two-page spreads joined by
//! [`spread::compose`]). Keys are bound to `script-message`s that come back
//! over the socket, so the controller can turn pages in reading direction,
//! append the next chapter to the playlist before the reader reaches the
//! end, load the previous chapter when stepping back past the first page,
//! and switch single/double layout without closing the window. Every page
//! change is reported to the caller, which saves the reading state.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use futures::StreamExt;
use futures::future::BoxFuture;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::net::unix::OwnedWriteHalf;

use ap0dl_core::error::{Error, Result};

use crate::spread::{self, PageLayout, ReadingDirection};

/// Frames still ahead of the current one before the next chapter is fetched.
const LOOKAHEAD_FRAMES: usize = 6;
/// Chapters kept loaded behind the current one; older ones are dropped.
const KEEP_BEHIND: usize = 2;
const POSITION_PROPERTY: u64 = 1;

/// Supplies chapter images to the viewer.
#[async_trait]
pub trait ChapterFeed: Send + Sync {
    /// Number of chapters the viewer may move through.
    fn chapter_count(&self) -> usize;
    /// Short chapter name for the title bar and OSD ("Chapter 12").
    fn label(&self, index: usize) -> String;
    /// Fetches chapter `index` into `directory` and returns its page images
    /// in reading order.
    async fn load(&self, index: usize, directory: &Path) -> Result<Vec<PathBuf>>;
}

#[derive(Debug, Clone)]
pub struct ViewerOptions {
    /// mpv binary (Linux/macOS mpv; the IPC socket is a Unix socket).
    pub mpv: String,
    /// Series title for the window title.
    pub title: String,
    /// Extra mpv arguments (`reader_args`).
    pub mpv_args: Vec<String>,
    pub direction: ReadingDirection,
    /// `ffmpeg` for joining spreads; `None` forces single-page layout.
    pub ffmpeg: Option<PathBuf>,
    /// Scratch directory for page images, spreads, the socket and input.conf.
    pub work_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewerSettings {
    pub layout: PageLayout,
    /// Show the first page of a chapter alone so the pairs shift by one.
    pub offset: bool,
}

/// Where the reader is: chapter index in the feed and zero-based page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub chapter: usize,
    pub page: usize,
    pub pages: usize,
    /// The frame shows the chapter's last page.
    pub last: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct Outcome {
    pub position: Position,
    pub settings: ViewerSettings,
}

#[derive(Debug, Clone)]
struct Frame {
    path: PathBuf,
    pages: Vec<usize>,
}

#[derive(Debug)]
struct Loaded {
    index: usize,
    directory: PathBuf,
    pages: Vec<PathBuf>,
    sizes: Vec<Option<(u32, u32)>>,
    settings: ViewerSettings,
    frames: Vec<Frame>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Jump {
    /// Next frame, once the next chapter is appended.
    Forward,
    /// Last frame of the previous chapter, once it is loaded.
    Back,
    /// First frame of this chapter.
    Start(usize),
}

/// mpv key bindings. Navigation goes through the controller (it knows when
/// the next chapter is still loading); `q` and closing the window quit.
const INPUT_CONF: &str = "\
RIGHT script-message ap0dl right
LEFT script-message ap0dl left
l script-message ap0dl right
h script-message ap0dl left
DOWN script-message ap0dl next
UP script-message ap0dl prev
j script-message ap0dl next
k script-message ap0dl prev
SPACE script-message ap0dl next
ENTER script-message ap0dl next
PGDWN script-message ap0dl next
PGUP script-message ap0dl prev
BS script-message ap0dl prev
WHEEL_DOWN script-message ap0dl next
WHEEL_UP script-message ap0dl prev
> script-message ap0dl next
< script-message ap0dl prev
n script-message ap0dl next-chapter
p script-message ap0dl prev-chapter
d script-message ap0dl layout
o script-message ap0dl offset
? script-message ap0dl help
q quit
Q quit
CLOSE_WIN quit
";

/// Shows the feed from `start_chapter`/`start_page` until mpv quits.
/// `on_change` runs on every page turn and settings change.
pub async fn view(
    feed: &dyn ChapterFeed,
    options: &ViewerOptions,
    settings: ViewerSettings,
    start_chapter: usize,
    start_page: usize,
    on_change: &mut dyn FnMut(Position, ViewerSettings),
) -> Result<Outcome> {
    let settings = ViewerSettings {
        layout: if options.ffmpeg.is_some() {
            settings.layout
        } else {
            PageLayout::Single
        },
        ..settings
    };
    std::fs::create_dir_all(&options.work_dir)?;
    let first = prepare(feed, options, settings, start_chapter).await?;
    let start_page = start_page.min(first.pages.len().saturating_sub(1));
    let start_frame = first
        .frames
        .iter()
        .position(|frame| frame.pages.contains(&start_page))
        .unwrap_or(0);
    let paths: Vec<String> = first
        .frames
        .iter()
        .map(|frame| frame.path.display().to_string())
        .collect();

    let socket = options.work_dir.join("mpv.sock");
    let input_conf = options.work_dir.join("input.conf");
    std::fs::write(&input_conf, INPUT_CONF)?;
    let _ = std::fs::remove_file(&socket);
    let title = format!("{} - {}", options.title, feed.label(start_chapter));
    let mut args =
        crate::reader::mpv_args(&options.mpv, &title, &BTreeMap::new(), &options.mpv_args);
    args.extend([
        format!("--input-ipc-server={}", socket.display()),
        format!("--input-conf={}", input_conf.display()),
        "--terminal=no".to_string(),
        "--autofit-larger=95%x95%".to_string(),
        format!("--playlist-start={start_frame}"),
    ]);
    args.extend(paths);

    let mut viewer = Viewer {
        feed,
        options,
        settings,
        chapters: vec![first],
        current: start_frame,
        writer: None,
        suppress: None,
        count_check: None,
        next_request: 100,
        pending: None,
        failed: HashSet::new(),
        shown_chapter: None,
        on_change,
    };
    let start = viewer.position().expect("first chapter has frames");
    (viewer.on_change)(start, settings);

    if which::which(&options.mpv).is_err() {
        return Err(Error::Player(format!(
            "{:?} was not found in PATH; install mpv or choose another --reader",
            options.mpv
        )));
    }
    let mut child = tokio::process::Command::new(&options.mpv)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|error| Error::Player(format!("could not launch {}: {error}", options.mpv)))?;

    let Some(stream) = connect(&socket, &mut child).await? else {
        // mpv exited before opening its socket (or is not a real mpv).
        return Ok(viewer.outcome(start));
    };
    let (read, write) = stream.into_split();
    viewer.writer = Some(write);
    let mut lines = BufReader::new(read).lines();
    let mut last = start;

    let session: Result<()> = async {
        viewer
            .send(json!([
                "observe_property",
                POSITION_PROPERTY,
                "playlist-pos"
            ]))
            .await?;
        viewer.help(5000).await?;
        let mut loading: Option<BoxFuture<'_, (usize, Result<Loaded>)>> = None;
        loop {
            if loading.is_none()
                && let Some(index) = viewer.wanted()
            {
                let settings = viewer.settings;
                tracing::debug!("loading chapter {index}");
                loading = Some(Box::pin(async move {
                    (index, prepare(feed, options, settings, index).await)
                }));
            }
            tokio::select! {
                line = lines.next_line() => match line {
                    Ok(Some(line)) => viewer.handle(&line).await?,
                    _ => break,
                },
                (index, result) = async { loading.as_mut().expect("guarded by is_some").await },
                    if loading.is_some() =>
                {
                    loading = None;
                    tracing::debug!("chapter {index} loaded: {:?}", result.as_ref().map(|chapter| chapter.frames.len()));
                    viewer.loaded(index, result).await?;
                }
                _ = child.wait() => break,
            }
            if let Some(position) = viewer.position() {
                last = position;
            }
        }
        Ok(())
    }
    .await;
    if tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .is_err()
    {
        let _ = child.kill().await;
    }
    let _ = std::fs::remove_file(&socket);
    session?;
    Ok(viewer.outcome(last))
}

async fn connect(socket: &Path, child: &mut tokio::process::Child) -> Result<Option<UnixStream>> {
    for _ in 0..200 {
        if let Ok(stream) = UnixStream::connect(socket).await {
            return Ok(Some(stream));
        }
        if child.try_wait()?.is_some() {
            return Ok(None);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Err(Error::Player(
        "mpv did not open its IPC socket within 10 seconds".to_string(),
    ))
}

/// Loads chapter `index` and lays out its frames.
async fn prepare(
    feed: &dyn ChapterFeed,
    options: &ViewerOptions,
    settings: ViewerSettings,
    index: usize,
) -> Result<Loaded> {
    let directory = options.work_dir.join(format!("chapter-{index:04}"));
    let pages = feed.load(index, &directory).await?;
    if pages.is_empty() {
        return Err(Error::download(format!(
            "{} has no pages",
            feed.label(index)
        )));
    }
    let sizes: Vec<Option<(u32, u32)>> = pages.iter().map(|page| spread::file_size(page)).collect();
    let frames = layout(options, settings, &directory, &pages, &sizes).await;
    Ok(Loaded {
        index,
        directory,
        pages,
        sizes,
        settings,
        frames,
    })
}

/// Frames for one chapter. Pairs ffmpeg cannot join are shown one by one.
async fn layout(
    options: &ViewerOptions,
    settings: ViewerSettings,
    directory: &Path,
    pages: &[PathBuf],
    sizes: &[Option<(u32, u32)>],
) -> Vec<Frame> {
    let groups = match options.ffmpeg {
        Some(_) => spread::frames(sizes, settings.layout, settings.offset),
        None => spread::frames(sizes, PageLayout::Single, false),
    };
    let built: Vec<Vec<Frame>> = futures::stream::iter(groups)
        .map(|group| async move {
            let single = |index: usize| Frame {
                path: pages[index].clone(),
                pages: vec![index],
            };
            let (Some(ffmpeg), [first, second]) = (options.ffmpeg.as_deref(), group.as_slice())
            else {
                return group.iter().map(|index| single(*index)).collect();
            };
            let output = spread::spread_path(directory, *first, *second, options.direction);
            let height = [sizes[*first], sizes[*second]]
                .iter()
                .flatten()
                .map(|(_, height)| *height)
                .max()
                .unwrap_or(1600);
            if output.exists()
                || spread::compose(
                    ffmpeg,
                    &pages[*first],
                    &pages[*second],
                    height,
                    options.direction,
                    &output,
                )
                .await
                .inspect_err(|error| tracing::warn!("{error}"))
                .is_ok()
            {
                vec![Frame {
                    path: output,
                    pages: group.clone(),
                }]
            } else {
                vec![single(*first), single(*second)]
            }
        })
        .buffered(4)
        .collect()
        .await;
    built.into_iter().flatten().collect()
}

struct Viewer<'a> {
    feed: &'a dyn ChapterFeed,
    options: &'a ViewerOptions,
    settings: ViewerSettings,
    /// Loaded chapters, contiguous and in order; their frames, concatenated,
    /// mirror mpv's playlist.
    chapters: Vec<Loaded>,
    /// Current playlist index.
    current: usize,
    writer: Option<OwnedWriteHalf>,
    /// While the playlist is rearranged, position events are ignored until
    /// the reply to this request id arrives.
    suppress: Option<u64>,
    /// Request id of the pending playlist-count check.
    count_check: Option<u64>,
    next_request: u64,
    pending: Option<Jump>,
    failed: HashSet<usize>,
    shown_chapter: Option<usize>,
    on_change: &'a mut dyn FnMut(Position, ViewerSettings),
}

impl Viewer<'_> {
    fn outcome(&self, position: Position) -> Outcome {
        Outcome {
            position,
            settings: self.settings,
        }
    }

    fn playlist_len(&self) -> usize {
        self.chapters
            .iter()
            .map(|chapter| chapter.frames.len())
            .sum()
    }

    fn first_index(&self) -> usize {
        self.chapters[0].index
    }

    fn last_index(&self) -> usize {
        self.chapters[self.chapters.len() - 1].index
    }

    /// (loaded chapter slot, frame) at playlist index `position`.
    fn locate(&self, mut position: usize) -> Option<(usize, usize)> {
        for (slot, chapter) in self.chapters.iter().enumerate() {
            if position < chapter.frames.len() {
                return Some((slot, position));
            }
            position -= chapter.frames.len();
        }
        None
    }

    fn first_frame_of(&self, index: usize) -> Option<usize> {
        let mut offset = 0;
        for chapter in &self.chapters {
            if chapter.index == index {
                return Some(offset);
            }
            offset += chapter.frames.len();
        }
        None
    }

    fn position(&self) -> Option<Position> {
        let (slot, frame) = self.locate(self.current)?;
        let chapter = &self.chapters[slot];
        let pages = &chapter.frames[frame].pages;
        Some(Position {
            chapter: chapter.index,
            page: pages[0],
            pages: chapter.pages.len(),
            last: pages.last() == Some(&(chapter.pages.len() - 1)),
        })
    }

    /// The chapter to fetch next: whatever a pending jump needs, else the
    /// next chapter once fewer than [`LOOKAHEAD_FRAMES`] frames remain.
    fn wanted(&self) -> Option<usize> {
        let count = self.feed.chapter_count();
        let next = (self.last_index() + 1 < count).then(|| self.last_index() + 1);
        let previous = self.first_index().checked_sub(1);
        match self.pending {
            Some(Jump::Back) => return previous,
            Some(Jump::Start(target)) if target < self.first_index() => return previous,
            Some(Jump::Start(target)) if target > self.last_index() => return next,
            Some(Jump::Forward) => return next,
            _ => {}
        }
        let ahead = self.playlist_len().saturating_sub(self.current + 1);
        next.filter(|index| ahead < LOOKAHEAD_FRAMES && !self.failed.contains(index))
    }

    async fn send(&mut self, command: Value) -> Result<()> {
        let Some(writer) = self.writer.as_mut() else {
            return Ok(());
        };
        tracing::debug!("mpv <- {command}");
        let mut line = serde_json::to_vec(&json!({ "command": command }))?;
        line.push(b'\n');
        writer.write_all(&line).await?;
        Ok(())
    }

    async fn osd(&mut self, text: &str, millis: u64) -> Result<()> {
        self.send(json!(["show-text", text, millis])).await
    }

    async fn help(&mut self, millis: u64) -> Result<()> {
        let (forward, back) = match self.options.direction {
            ReadingDirection::Rtl => ("Left", "Right"),
            ReadingDirection::Ltr => ("Right", "Left"),
        };
        let text = format!(
            "{forward}/Space/Down: next page   {back}/Backspace/Up: previous\n\
             n/p: next/previous chapter   d: single/double   o: shift pairs\n\
             f: fullscreen   ?: help   q: quit"
        );
        self.osd(&text, millis).await
    }

    /// Starts ignoring position events until the matching reply arrives.
    fn begin_rearrange(&mut self) -> u64 {
        self.next_request += 1;
        self.suppress = Some(self.next_request);
        self.next_request
    }

    /// Asks mpv for its playlist length (checked against the model) and
    /// then its position, whose reply ends the suppression.
    async fn end_rearrange(&mut self, request: u64) -> Result<()> {
        self.next_request += 1;
        self.count_check = Some(self.next_request);
        let check = self.next_request;
        self.request(json!(["get_property", "playlist-count"]), check)
            .await?;
        self.request(json!(["get_property", "playlist-pos"]), request)
            .await
    }

    async fn request(&mut self, command: Value, request: u64) -> Result<()> {
        let Some(writer) = self.writer.as_mut() else {
            return Ok(());
        };
        let mut line = serde_json::to_vec(&json!({ "command": command, "request_id": request }))?;
        line.push(b'\n');
        writer.write_all(&line).await?;
        Ok(())
    }

    /// Replaces mpv's whole playlist with the model's frames. Only used when
    /// an incremental rearrangement left the two out of step.
    async fn resync(&mut self) -> Result<()> {
        let paths: Vec<String> = self
            .chapters
            .iter()
            .flat_map(|chapter| &chapter.frames)
            .map(|frame| frame.path.display().to_string())
            .collect();
        let Some(first) = paths.first().cloned() else {
            return Ok(());
        };
        let request = self.begin_rearrange();
        self.send(json!(["loadfile", first, "replace"])).await?;
        for path in &paths[1..] {
            self.send(json!(["loadfile", path, "append"])).await?;
        }
        self.send(json!(["playlist-play-index", self.current]))
            .await?;
        self.end_rearrange(request).await
    }

    async fn handle(&mut self, line: &str) -> Result<()> {
        let Ok(message) = serde_json::from_str::<Value>(line) else {
            return Ok(());
        };
        tracing::debug!("mpv -> {line}");
        if let Some(request) = message.get("request_id").and_then(Value::as_u64) {
            if self.count_check == Some(request) {
                self.count_check = None;
                let count = message["data"].as_u64().unwrap_or(0) as usize;
                if count != self.playlist_len() {
                    tracing::warn!(
                        "mpv playlist has {count} entries, expected {}; rebuilding it",
                        self.playlist_len()
                    );
                    return self.resync().await;
                }
            } else if self.suppress == Some(request) {
                self.suppress = None;
                if let Some(position) = message["data"].as_u64() {
                    self.moved(position as usize).await?;
                }
            }
            return Ok(());
        }
        match message["event"].as_str() {
            Some("property-change") if message["id"].as_u64() == Some(POSITION_PROPERTY) => {
                if self.suppress.is_none()
                    && let Some(position) = message["data"].as_u64()
                {
                    self.moved(position as usize).await?;
                }
            }
            Some("client-message") => {
                let args = message["args"].as_array().cloned().unwrap_or_default();
                if args.first().and_then(Value::as_str) == Some("ap0dl")
                    && let Some(action) = args.get(1).and_then(Value::as_str)
                {
                    self.action(action).await?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    async fn moved(&mut self, position: usize) -> Result<()> {
        if position >= self.playlist_len() {
            return Ok(());
        }
        self.current = position;
        let Some(now) = self.position() else {
            return Ok(());
        };
        (self.on_change)(now, self.settings);

        let label = self.feed.label(now.chapter);
        let shown = match self.locate(position) {
            Some((slot, frame)) if self.chapters[slot].frames[frame].pages.len() == 2 => {
                format!("{}-{}", now.page + 1, now.page + 2)
            }
            _ => (now.page + 1).to_string(),
        };
        let title = format!("{} - {label} ({shown}/{})", self.options.title, now.pages);
        self.send(json!(["set_property", "force-media-title", title]))
            .await?;
        if self.shown_chapter != Some(now.chapter) {
            if self.shown_chapter.is_some() {
                self.osd(&label, 2000).await?;
            }
            self.shown_chapter = Some(now.chapter);
        }
        self.evict().await
    }

    /// Drops chapters more than [`KEEP_BEHIND`] behind the current one.
    async fn evict(&mut self) -> Result<()> {
        let Some((slot, _)) = self.locate(self.current) else {
            return Ok(());
        };
        if slot <= KEEP_BEHIND {
            return Ok(());
        }
        let drop = slot - KEEP_BEHIND;
        let frames: usize = self.chapters[..drop]
            .iter()
            .map(|chapter| chapter.frames.len())
            .sum();
        let request = self.begin_rearrange();
        for _ in 0..frames {
            self.send(json!(["playlist-remove", 0])).await?;
        }
        for chapter in self.chapters.drain(..drop) {
            let _ = std::fs::remove_dir_all(&chapter.directory);
        }
        self.current -= frames;
        self.end_rearrange(request).await
    }

    async fn action(&mut self, action: &str) -> Result<()> {
        let forward = match (action, self.options.direction) {
            ("next", _) | ("left", ReadingDirection::Rtl) | ("right", ReadingDirection::Ltr) => {
                Some(true)
            }
            ("prev", _) | ("right", ReadingDirection::Rtl) | ("left", ReadingDirection::Ltr) => {
                Some(false)
            }
            _ => None,
        };
        match (forward, action) {
            (Some(true), _) => self.forward().await,
            (Some(false), _) => self.back().await,
            (None, "next-chapter") => self.jump_chapter(1).await,
            (None, "prev-chapter") => self.jump_chapter(-1).await,
            (None, "layout") => {
                if self.options.ffmpeg.is_none() {
                    return self.osd("Double page needs ffmpeg in PATH", 3000).await;
                }
                self.settings.layout = self.settings.layout.toggled();
                self.relayout().await
            }
            (None, "offset") => {
                self.settings.offset = !self.settings.offset;
                if self.settings.layout == PageLayout::Single {
                    return self
                        .osd("Pair shift applies in double page layout", 2000)
                        .await;
                }
                self.relayout().await
            }
            (None, "help") => self.help(6000).await,
            _ => Ok(()),
        }
    }

    async fn forward(&mut self) -> Result<()> {
        if self.current + 1 < self.playlist_len() {
            return self.send(json!(["playlist-next", "force"])).await;
        }
        let next = self.last_index() + 1;
        if next < self.feed.chapter_count() {
            self.failed.remove(&next);
            self.pending = Some(Jump::Forward);
            let text = format!("Loading {}...", self.feed.label(next));
            return self.osd(&text, 10_000).await;
        }
        self.osd("End of the last chapter (q quits)", 3000).await
    }

    async fn back(&mut self) -> Result<()> {
        if self.current > 0 {
            return self.send(json!(["playlist-prev", "force"])).await;
        }
        match self.first_index().checked_sub(1) {
            Some(previous) => {
                self.failed.remove(&previous);
                self.pending = Some(Jump::Back);
                let text = format!("Loading {}...", self.feed.label(previous));
                self.osd(&text, 10_000).await
            }
            None => self.osd("First page", 1500).await,
        }
    }

    async fn jump_chapter(&mut self, delta: isize) -> Result<()> {
        let Some(now) = self.position() else {
            return Ok(());
        };
        let Some(target) = now
            .chapter
            .checked_add_signed(delta)
            .filter(|target| *target < self.feed.chapter_count())
        else {
            let text = if delta > 0 {
                "This is the last chapter"
            } else {
                "This is the first chapter"
            };
            return self.osd(text, 2000).await;
        };
        if let Some(frame) = self.first_frame_of(target) {
            return self.send(json!(["playlist-play-index", frame])).await;
        }
        self.failed.remove(&target);
        self.pending = Some(Jump::Start(target));
        let text = format!("Loading {}...", self.feed.label(target));
        self.osd(&text, 10_000).await
    }

    async fn loaded(&mut self, index: usize, result: Result<Loaded>) -> Result<()> {
        let mut chapter = match result {
            Ok(chapter) => chapter,
            Err(error) => {
                self.failed.insert(index);
                self.pending = None;
                let text = format!("{} failed: {error}", self.feed.label(index));
                return self.osd(&text, 5000).await;
            }
        };
        if chapter.settings != self.settings {
            chapter.frames = layout(
                self.options,
                self.settings,
                &chapter.directory,
                &chapter.pages,
                &chapter.sizes,
            )
            .await;
            chapter.settings = self.settings;
        }
        let paths: Vec<String> = chapter
            .frames
            .iter()
            .map(|frame| frame.path.display().to_string())
            .collect();
        if index == self.last_index() + 1 {
            for path in &paths {
                self.send(json!(["loadfile", path, "append"])).await?;
            }
            self.chapters.push(chapter);
        } else if index + 1 == self.first_index() {
            // Append, then move each new entry to the front: the frame on
            // screen is never reloaded, only its index shifts.
            let request = self.begin_rearrange();
            let length = self.playlist_len();
            for path in &paths {
                self.send(json!(["loadfile", path, "append"])).await?;
            }
            for offset in 0..paths.len() {
                self.send(json!(["playlist-move", length + offset, offset]))
                    .await?;
            }
            self.current += paths.len();
            self.chapters.insert(0, chapter);
            self.end_rearrange(request).await?;
        } else {
            return Ok(());
        }

        match self.pending {
            Some(Jump::Forward) if self.current + 1 < self.playlist_len() => {
                self.pending = None;
                self.send(json!(["playlist-next", "force"])).await?;
            }
            Some(Jump::Back) if self.current > 0 => {
                self.pending = None;
                self.send(json!(["playlist-play-index", self.current - 1]))
                    .await?;
            }
            Some(Jump::Start(target)) => {
                if let Some(frame) = self.first_frame_of(target) {
                    self.pending = None;
                    self.send(json!(["playlist-play-index", frame])).await?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Re-lays out every loaded chapter after a layout or offset change and
    /// rebuilds mpv's playlist around the page on screen.
    async fn relayout(&mut self) -> Result<()> {
        let Some(now) = self.position() else {
            return Ok(());
        };
        for slot in 0..self.chapters.len() {
            let chapter = &self.chapters[slot];
            let frames = layout(
                self.options,
                self.settings,
                &chapter.directory,
                &chapter.pages,
                &chapter.sizes,
            )
            .await;
            let chapter = &mut self.chapters[slot];
            chapter.frames = frames;
            chapter.settings = self.settings;
        }
        let mut target = 0;
        let mut paths = Vec::new();
        for chapter in &self.chapters {
            for frame in &chapter.frames {
                if chapter.index == now.chapter && frame.pages.contains(&now.page) {
                    target = paths.len();
                }
                paths.push(frame.path.display().to_string());
            }
        }

        let request = self.begin_rearrange();
        self.send(json!(["playlist-clear"])).await?;
        for path in &paths {
            self.send(json!(["loadfile", path, "append"])).await?;
        }
        self.send(json!(["playlist-play-index", target + 1]))
            .await?;
        self.send(json!(["playlist-remove", 0])).await?;
        self.current = target;
        self.end_rearrange(request).await?;

        let mut text = match self.settings.layout {
            PageLayout::Single => "Single page".to_string(),
            PageLayout::Double => "Double page".to_string(),
        };
        if self.settings.offset && self.settings.layout == PageLayout::Double {
            text.push_str(" (first page alone)");
        }
        self.osd(&text, 2000).await
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    /// Chapters of portrait pages made by ffmpeg, so the viewer can run
    /// without any network.
    struct FakeFeed {
        pages: Vec<usize>,
        image: PathBuf,
    }

    #[async_trait]
    impl ChapterFeed for FakeFeed {
        fn chapter_count(&self) -> usize {
            self.pages.len()
        }

        fn label(&self, index: usize) -> String {
            format!("Chapter {}", index + 1)
        }

        async fn load(&self, index: usize, directory: &Path) -> Result<Vec<PathBuf>> {
            std::fs::create_dir_all(directory)?;
            (0..self.pages[index])
                .map(|page| {
                    let path = directory.join(format!("{page:03}.png"));
                    std::fs::copy(&self.image, &path)?;
                    Ok(path)
                })
                .collect()
        }
    }

    async fn key(socket: &Path, name: &str) {
        let mut stream = UnixStream::connect(socket).await.unwrap();
        let line = format!("{}\n", json!({ "command": ["keypress", name] }));
        stream.write_all(line.as_bytes()).await.unwrap();
    }

    async fn settle(seen: &Arc<Mutex<Vec<Position>>>, count: usize) -> Position {
        for _ in 0..100 {
            {
                let seen = seen.lock().unwrap();
                if seen.len() > count {
                    return *seen.last().unwrap();
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("the viewer did not move");
    }

    /// Real mpv (no window) driven through its socket: spreads, stepping
    /// back into an unloaded chapter, running on into the next chapters,
    /// dropping old chapters, and switching to single pages.
    #[tokio::test]
    #[ignore = "needs mpv and ffmpeg"]
    async fn mpv_session_reads_across_chapters() {
        let work_dir =
            std::env::temp_dir().join(format!("ap0dl-viewer-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&work_dir);
        std::fs::create_dir_all(&work_dir).unwrap();
        let image = work_dir.join("portrait.png");
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=gray:s=80x120",
            ])
            .args(["-frames:v", "1"])
            .arg(&image)
            .status()
            .unwrap();
        assert!(status.success());

        let feed = FakeFeed {
            pages: vec![3; 6],
            image,
        };
        let options = ViewerOptions {
            mpv: "mpv".to_string(),
            title: "Test".to_string(),
            mpv_args: vec!["--vo=null".to_string()],
            direction: ReadingDirection::Rtl,
            ffmpeg: which::which("ffmpeg").ok(),
            work_dir: work_dir.join("view"),
        };
        let settings = ViewerSettings {
            layout: PageLayout::Double,
            offset: false,
        };
        let seen = Arc::new(Mutex::new(Vec::new()));
        let record = seen.clone();
        let socket = options.work_dir.join("mpv.sock");

        let driver = async {
            let step = async |name: &str| {
                let before = seen.lock().unwrap().len();
                key(&socket, name).await;
                settle(&seen, before).await
            };
            let start = settle(&seen, 0).await;
            assert_eq!((start.chapter, start.page), (2, 0));
            // Wait for mpv to open its socket and show the first spread.
            for _ in 0..100 {
                if UnixStream::connect(&socket).await.is_ok() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
            // Right is "back" in RTL: chapter 2 is loaded and shown at its end.
            let back = step("RIGHT").await;
            assert_eq!((back.chapter, back.page, back.last), (1, 2, true));
            assert_eq!(step("LEFT").await.chapter, 2);
            assert_eq!(step("SPACE").await.page, 2);
            let mut now = step("LEFT").await;
            assert_eq!((now.chapter, now.page), (3, 0));
            for _ in 0..4 {
                now = step("LEFT").await;
            }
            assert_eq!((now.chapter, now.page), (5, 0));
            let single = step("d").await;
            assert_eq!((single.chapter, single.page), (5, 0));
            assert_eq!(step("DOWN").await.page, 1);
            assert_eq!(step("p").await.chapter, 4);
            key(&socket, "q").await;
        };
        let mut on_change = |position, _| record.lock().unwrap().push(position);
        let session = view(&feed, &options, settings, 2, 0, &mut on_change);
        let (outcome, ()) = tokio::join!(session, driver);
        let outcome = outcome.unwrap();
        assert_eq!(outcome.settings.layout, PageLayout::Single);
        assert_eq!(outcome.position.chapter, 4);
        // Chapters 1 and 2 were dropped once chapter 5 was on screen.
        assert!(!options.work_dir.join("chapter-0001").exists());
        std::fs::remove_dir_all(&work_dir).unwrap();
    }
}
