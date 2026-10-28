use std::{
	io::{Cursor, Read, Seek},
	sync::LazyLock,
	time::Duration,
};

use anyhow::{Context, bail};
use dashmap::DashSet;
use itertools::Itertools;
use stream_download::{
	Settings, StreamDownload,
	process::{CommandBuilder, FfmpegConvertAudioCommand, ProcessStreamParams, YtDlpCommand},
	storage::temp::TempStorageProvider,
};
use youtube_dl::{YoutubeDl, YoutubeDlOutput};

use crate::{
	data::{audio_cache_dir, is_favorited},
	yt::youtube_link,
};

static DOWNLOADING: LazyLock<DashSet<String>> = LazyLock::new(DashSet::new);

struct DownloadGuard(String);

impl Drop for DownloadGuard {
	fn drop(&mut self) {
		DOWNLOADING.remove(&self.0);
	}
}

pub async fn get_audio(id: &str) -> anyhow::Result<AudioSource> {
	if let Ok(src) = cacache::read(audio_cache_dir(), id).await {
		return Ok(AudioSource::Local(Cursor::new(src)));
	}

	let yt_link = youtube_link(id);

	// `insert` returns true if key used to be vacant
	if is_favorited(id) && DOWNLOADING.insert(id.to_owned()) {
		tokio::spawn(fetch_inner(id.to_owned(), yt_link.clone()));
	}

	let reader = remote_reader(&yt_link).await?;
	Ok(AudioSource::Remote(reader))
}

pub async fn remove(id: &str) {
	let _ = cacache::remove(audio_cache_dir(), id).await;
}

pub fn fetch(id: &str) {
	let dir = audio_cache_dir();
	match cacache::metadata_sync(&dir, id) {
		// Metadata existing is not enough of a guarentee and `exists` takes integrity, not id
		Ok(Some(md)) if cacache::exists_sync(&dir, &md.integrity) => {}
		_ => {
			if DOWNLOADING.insert(id.to_owned()) {
				let id = id.to_owned();
				let yt_link = youtube_link(&id);
				tokio::spawn(
					async move { fetch_inner(id.to_owned(), yt_link).await },
				);
			}
		}
	}
}

async fn fetch_inner(id: String, url: String) {
	let _guard = DownloadGuard(id.clone());
	println!("Downloading audio: {id}");

	// try block at home
	let res = (async || -> anyhow::Result<Vec<u8>> {
		let reader = remote_reader(&url).await?;

		let (tx, rx) = tokio::sync::oneshot::channel();
		std::thread::spawn(|| {
			let mut stream = reader.bytes();
			let b = Vec::with_capacity(stream.size_hint().0);
			let source = stream.fold_ok(b, |mut acc, x| {
				acc.push(x);
				acc
			});

			let _ = tx.send(source);
		});

		let source = rx.await.context("Download thread dropped")??;
		Ok(source)
	})()
	.await;

	let bytes = match res {
		Ok(r) => r,
		Err(e) => {
			eprintln!("Download failed for {id}: {e}");
			return;
		}
	};

	if let Err(e) = cacache::write(audio_cache_dir(), &id, bytes).await {
		eprintln!("Cache write failed for {id}: {e}");
		return;
	}

	println!("Downloaded audio: {id}");
}

pub enum AudioSource {
	Remote(StreamDownload<TempStorageProvider>),
	Local(Cursor<Vec<u8>>),
}

impl Read for AudioSource {
	fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
		match self {
			AudioSource::Remote(s) => s.read(buf),
			AudioSource::Local(s) => s.read(buf),
		}
	}
}

impl Seek for AudioSource {
	fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
		match self {
			AudioSource::Remote(s) => s.seek(pos),
			AudioSource::Local(s) => s.seek(pos),
		}
	}
}

impl AudioSource {
	pub fn len(&self) -> u64 {
		match self {
			AudioSource::Remote(s) => s
				.content_length()
				.expect("StreamDownload: Content length could is None."),
			AudioSource::Local(s) => s.get_ref().len() as u64,
		}
	}
}

// Snippet taken and modified from: https://github.com/aschey/stream-download-rs/blob/main/examples/examples/yt_dlp.rs
async fn remote_reader(yt_link: &str) -> anyhow::Result<StreamDownload<TempStorageProvider>> {
	// These two formats are the same, but yt-dlp and ffmpeg use different terminology
	const YT_DLP_FORMAT: &str = "m4a";
	const FFMPEG_FORMAT: &str = "adts";

	let output = YoutubeDl::new(yt_link)
		.extract_audio(true)
		.run_async()
		.await?;

	let found_format = match output {
		YoutubeDlOutput::SingleVideo(video) => {
			let formats = video.formats.unwrap();
			let worst_score = 0.0;
			// find best format, higher is better
			formats.into_iter()
				.filter(|f| f.ext.as_deref() == Some(YT_DLP_FORMAT))
				.reduce(|best, format| {
					match format.quality.unwrap_or(worst_score)
						> best.quality.unwrap_or(worst_score)
					{
						true => format,
						false => best,
					}
				})
		}
		YoutubeDlOutput::Playlist(_) => bail!("Expected video, found playlist."),
	};
	let cmd = YtDlpCommand::new(yt_link).extract_audio(true);

	let params = if let Some(format) = found_format {
		// Prefer the explicit format ID since this insures the format used will match
		// the filesize.
		let format_id = format
			.format_id
			.unwrap_or_else(|| YT_DLP_FORMAT.to_string());
		let params = ProcessStreamParams::new(cmd.format(format_id))?;
		if let Some(size) = format.filesize {
			params.content_length(size as u64)
		} else {
			params
		}
	} else {
		// yt-dlp can handle format conversion, but if we want to stream it directly from stdout,
		// we have to pipe the raw output to ffmpeg ourselves.
		let builder = CommandBuilder::new(cmd)
			.pipe(FfmpegConvertAudioCommand::new(FFMPEG_FORMAT));
		ProcessStreamParams::new(builder)?
	};

	static PREFECTH_AMOUNT: u64 = 4 * 1024;
	let settings = Settings::default()
		// Sometimes it may take a while for ffmpeg to output a new chunk, so we can bump up the
		// retry timeout to be safe.
		.retry_timeout(Duration::from_secs(30))
		.prefetch_bytes(PREFECTH_AMOUNT);

	let reader =
		StreamDownload::new_process(params, TempStorageProvider::new(), settings).await?;

	Ok(reader)
}
