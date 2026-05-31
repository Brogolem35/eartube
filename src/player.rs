use std::{sync::LazyLock, time::Duration};

use anyhow::{Context, Ok, Result};
use rodio::{Decoder, MixerDeviceSink, Source};
use tokio::task::JoinHandle;

use crate::audio_cache;

static SINK_HANDLE: LazyLock<MixerDeviceSink> =
	LazyLock::new(|| rodio::DeviceSinkBuilder::open_default_sink().unwrap());

pub struct Player {
	pub inner: rodio::Player,
	pub duration: Duration,
}

impl Player {
	pub fn new(url: &str, volume: f32) -> JoinHandle<Result<Self>> {
		let url = url.to_owned();
		tokio::spawn(async move {
			let reader = audio_cache::get_audio(&url).await?;
			let len_hint = reader.len();

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
				player.set_volume(volume);

				Ok(Self {
					inner: player,
					duration,
				})
			})
			.await?
		})
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

		self.seek(res)
	}

	pub fn seek_backward(&self, amount: f32) -> Result<()> {
		let amount = Duration::from_secs_f32(amount);
		let res = self.get_pos().saturating_sub(amount);

		self.seek(res)
	}

	pub fn seek(&self, value: Duration) -> Result<()> {
		self.inner.try_seek(value)?;
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

#[derive(Default)]
pub enum PlayerState {
	#[default]
	None,
	Loading(JoinHandle<Result<Player>>),
	Loaded(Player),
}

impl PlayerState {
	pub fn get_player(&self) -> Option<&Player> {
		match self {
			PlayerState::None => None,
			PlayerState::Loading(_) => None,
			PlayerState::Loaded(p) => Some(p),
		}
	}

	pub async fn try_finish(&mut self) -> Option<Result<Player>> {
		match self {
			PlayerState::Loading(h) if h.is_finished() => {
				let res = h.await.ok();
				*self = PlayerState::None;
				res
			}
			_ => None,
		}
	}
}

impl Drop for PlayerState {
	fn drop(&mut self) {
		match self {
			PlayerState::Loading(h) => h.abort(),
			_ => {}
		}
	}
}
