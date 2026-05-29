use std::{
	io::{Cursor, Read, Seek},
	sync::LazyLock,
};

use dashmap::DashSet;
use stream_download::{Settings, StreamDownload, storage::temp::TempStorageProvider};

use crate::{
	data::{audio_cache_dir, is_favorited},
	get_stream_url,
	playback::youtube_link,
};

static DOWNLOADING: LazyLock<DashSet<String>> = LazyLock::new(DashSet::new);

pub async fn get_audio(id: &str) -> anyhow::Result<AudioSource> {
	if let Ok(src) = cacache::read(audio_cache_dir(), id).await {
		return Ok(AudioSource::Local(Cursor::new(src)));
	}

	static PREFECTH_AMOUNT: u64 = 4 * 1024;
	let yt_link = youtube_link(id);
	let stream_url = get_stream_url(&yt_link).await?;

	// `insert` returns true if key used to be vacant
	if is_favorited(id) && DOWNLOADING.insert(id.to_owned()) {
		tokio::spawn(fetch_inner(id.to_owned(), stream_url.clone()));
	}

	let settings = Settings::default().prefetch_bytes(PREFECTH_AMOUNT);
	let reader =
		StreamDownload::new_http(stream_url.parse()?, TempStorageProvider::new(), settings)
			.await?;
	Ok(AudioSource::Remote(reader))
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
				tokio::spawn(async move {
					let url = get_stream_url(&yt_link).await.unwrap();
					fetch_inner(id.to_owned(), url.to_owned()).await
				});
			}
		}
	}
}

async fn fetch_inner(id: String, url: String) {
	println!("Downloading audio: {}", &id);
	if let Ok(b) = reqwest::get(url).await
		&& let Ok(b) = b.bytes().await
	{
		let _ = cacache::write(audio_cache_dir(), &id, b).await;
		DOWNLOADING.remove(&id);
		println!("Downloaded audio: {}", &id);
	}
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
