use std::{sync::LazyLock, time::Duration};

use anyhow::{Context, Ok, Result};
use rodio::{Decoder, MixerDeviceSink, Source};
use stream_download::{Settings, StreamDownload, storage::temp::TempStorageProvider};

static SINK_HANDLE: LazyLock<MixerDeviceSink> =
	LazyLock::new(|| rodio::DeviceSinkBuilder::open_default_sink().unwrap());

pub struct Player {
	pub inner: rodio::Player,
	pub duration: Duration,
}

impl Player {
	pub async fn new(url: &str) -> Result<Self> {
		static PREFECTH_AMOUNT: u64 = 4 * 1024;

		let settings = Settings::default().prefetch_bytes(PREFECTH_AMOUNT);
		let reader = StreamDownload::new_http(
			url.parse()?,
			TempStorageProvider::new(),
			settings,
		)
		.await?;
		let len_hint = reader
			.content_length()
			.context("StreamDownload: Content length could is None.")?;

		tokio::task::spawn_blocking(move || {
			let source = Decoder::builder()
				.with_seekable(true)
				.with_byte_len(len_hint)
				.with_coarse_seek(true)
				.with_data(reader)
				.build()?;

			let duration = source
				.total_duration()
				.context("Decoder: Unknown length of stream.")?;
			let player = rodio::Player::connect_new(SINK_HANDLE.mixer());
			player.append(source);

			Ok(Self {
				inner: player,
				duration,
			})
		})
		.await?
	}

	#[allow(unused)]
	pub fn get_volume(&self) -> f32 {
		self.inner.volume()
	}

	#[allow(unused)]
	pub fn set_volume(&self, value: f32) {
		self.inner.set_volume(value);
	}

	#[allow(unused)]
	pub fn get_time_rem(&self) -> Duration {
		self.duration - self.inner.get_pos()
	}

	pub fn seek_forward(&self, amount: f32) -> Result<()> {
		let amount = Duration::from_secs_f32(amount);
		let res = (self.get_pos() + amount).min(self.duration);

		self.inner.try_seek(res)?;
		Ok(())
	}

	pub fn seek_backward(&self, amount: f32) -> Result<()> {
		let amount = Duration::from_secs_f32(amount);
		let res = self.get_pos().saturating_sub(amount);

		self.inner.try_seek(res)?;
		Ok(())
	}

	/// Pauses playback.
	pub fn pause(&self) {
		self.inner.pause();
	}

	/// Resumes playback.
	pub fn unpause(&self) {
		self.inner.play();
	}

	pub fn finished(&self) -> bool {
		self.inner.empty()
	}

	pub fn get_pos(&self) -> Duration {
		self.inner.get_pos()
	}
}
