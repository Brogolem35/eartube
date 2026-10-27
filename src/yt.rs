use std::sync::LazyLock;

use anyhow::Context;
use rustypipe::{client::RustyPipe, model::TrackItem, report::FileReporter};
use serde::Deserialize;
use tokio::join;

use crate::data;

static RP_CLIENT: LazyLock<RustyPipe> = LazyLock::new(|| {
	RustyPipe::builder()
		.storage_dir(data::cache_dir())
		.reporter(Box::new(FileReporter::new(data::reporter_dir())))
		.build()
		.unwrap()
});

#[derive(Clone, Debug, Default)]
pub struct YtSearch {
	pub tracks: Vec<TrackItem>,
	pub videos: Vec<TrackItem>,
}

pub async fn search(search_text: &str) -> anyhow::Result<YtSearch> {
	let q = RP_CLIENT.query();
	let (tracks, videos) = join!(
		q.music_search_tracks(search_text),
		q.music_search_videos(search_text)
	);

	let res = YtSearch {
		tracks: tracks?.items.items,
		videos: videos?.items.items,
	};
	Ok(res)
}

pub async fn new_radio(track: TrackItem) -> anyhow::Result<Vec<TrackItem>> {
	let q = RP_CLIENT.query();
	let mut radio = q.music_radio_track(&track.id).await?.items;
	radio.insert(0, track);

	Ok(radio)
}

pub async fn get_stream_url(youtube_url: &str) -> anyhow::Result<String> {
	#[derive(Debug, Deserialize)]
	struct YtDlpJson {
		url: String,
	}

	let output = tokio::process::Command::new("yt-dlp")
		.args(["-f", "ba[ext=m4a]", "--dump-single-json", youtube_url])
		.output()
		.await?;

	if !output.status.success() {
		anyhow::bail!("yt-dlp failed: {}", String::from_utf8_lossy(&output.stderr));
	}

	let json: YtDlpJson =
		serde_json::from_slice(&output.stdout).context("failed to parse yt-dlp json")?;

	Ok(json.url)
}

pub fn youtube_link(id: &str) -> String {
	format!("https://music.youtube.com/watch?v={}", id)
}
