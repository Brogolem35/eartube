use std::{fmt::Debug, time::Duration};

use rustypipe::model::TrackItem;
use tokio::{
	select,
	sync::mpsc::{UnboundedReceiver, UnboundedSender},
};

use crate::{get_stream_url, player::Player};

#[derive(Default)]
pub struct Playlist {
	player: Option<Player>,
	list: Vec<TrackItem>,
	index: Option<usize>,
	pause: bool,
}

impl Playlist {
	pub fn new() -> Self {
		Self {
			player: None,
			list: Vec::new(),
			index: None,
			pause: true,
		}
	}

	pub async fn play(&mut self) -> anyhow::Result<()> {
		if let Some(ref player) = self.player
			&& !player.finished()
		{
			return Ok(());
		}
		if self.is_empty() {
			self.index.take();
			self.player.take();
			return Ok(());
		}

		let index = match self.index {
			Some(i) => (i + 1).min(self.len() - 1),
			None => 0,
		};

		let track_item = self
			.get_track(index)
			.expect("Playlist index is greater than list size.");
		let yt_link = youtube_link(&track_item.id);
		let stream_url = get_stream_url(&yt_link).await?;

		self.player = Some(Player::new(&stream_url).await?);
		self.index = Some(index);

		Ok(())
	}

	pub fn finished(&self) -> bool {
		match &self.player {
			Some(p) => p.finished(),
			None => true,
		}
	}

	pub fn len(&self) -> usize {
		self.list.len()
	}

	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}

	pub fn set_list(&mut self, list: Vec<TrackItem>) {
		self.list = list;
		self.index.take();
		self.player.take();
		self.pause = false;
	}

	pub fn seek_forward(&mut self) -> anyhow::Result<()> {
		match &mut self.player {
			Some(p) => p.seek_forward(5.0),
			None => Ok(()),
		}
	}

	pub fn seek_backward(&mut self) -> anyhow::Result<()> {
		match &mut self.player {
			Some(p) => p.seek_backward(5.0),
			None => Ok(()),
		}
	}

	pub fn seek(&mut self, value: Duration) -> anyhow::Result<()> {
		match &mut self.player {
			Some(p) => p.seek(value),
			None => Ok(()),
		}
	}

	pub fn toggle_pause(&mut self) -> anyhow::Result<()> {
		self.pause = !self.pause;
		match self.pause {
			true => {
				if let Some(ref p) = self.player {
					p.pause();
				}
			}
			false => {
				if let Some(ref p) = self.player {
					p.unpause();
				}
			}
		}

		Ok(())
	}

	pub fn skip_next(&mut self) {
		// This will make `finished` true, thus skipping.
		self.player.take();
	}

	pub fn skip_prev(&mut self) {
		// Because ending the player will skip to next, I decrement the index by 2 instead of 1.
		// This is ugly and we have to live with it.
		self.index = match self.index {
			Some(i) => i.checked_sub(2),
			None => None,
		};
		self.player.take();
	}

	pub fn skip_to(&mut self, index: usize) {
		self.index = index.checked_sub(1);
		self.player.take();
	}

	pub fn playlist_view(&self) -> PlaylistView {
		PlaylistView {
			list: self.list.clone(),
			index: self.index,
			player: self.player_view(),
		}
	}

	pub fn player_view(&self) -> PlayerView {
		let pref = self.player.as_ref();
		PlayerView {
			pause: self.pause,
			pos: pref.map(|p| p.get_pos()).unwrap_or_default(),
			length: pref.map(|p| p.duration).unwrap_or_default(),
		}
	}

	pub fn get_track(&self, index: usize) -> Option<&TrackItem> {
		self.list.get(index)
	}
}

impl Debug for Playlist {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Playlist")
			.field("player", &self.player.is_some())
			.field("list", &self.list)
			.field("index", &self.index)
			.field("pause", &self.pause)
			.finish()
	}
}

pub enum PlaybackCommand {
	LoadPlaylist(Vec<TrackItem>),
	TogglePause,
	SkipNext,
	SkipPrev,
	SkipTo(usize),
	SeekForward,
	SeekBackward,
	Seek(Duration),
}

pub enum PlaybackEvent {
	PlaylistUpdated(PlaylistView),
	PlayerUpdated(PlayerView),
}

pub async fn playback_loop(
	mut rx: UnboundedReceiver<PlaybackCommand>,
	tx: UnboundedSender<PlaybackEvent>,
) {
	let mut pl = Playlist::new();

	loop {
		select! {
			Some(cmd) = rx.recv() => {
				playback_command(&mut pl, cmd, &tx).await;
			}
			_ = tokio::time::sleep(Duration::from_millis(100)) => {
				playback_idle_tick(&mut pl, &tx).await;
			}
		}
	}
}

pub async fn playback_command(
	pl: &mut Playlist,
	cmd: PlaybackCommand,
	tx: &UnboundedSender<PlaybackEvent>,
) {
	match cmd {
		PlaybackCommand::LoadPlaylist(list) => {
			pl.set_list(list);
			tx.send(PlaybackEvent::PlaylistUpdated(pl.playlist_view()))
				.unwrap();
		}
		PlaybackCommand::TogglePause => {
			pl.toggle_pause().unwrap();
			tx.send(PlaybackEvent::PlayerUpdated(pl.player_view()))
				.unwrap();
		}
		PlaybackCommand::SkipNext => {
			pl.skip_next();
			tx.send(PlaybackEvent::PlaylistUpdated(pl.playlist_view()))
				.unwrap();
		}
		PlaybackCommand::SkipPrev => {
			pl.skip_prev();
			tx.send(PlaybackEvent::PlaylistUpdated(pl.playlist_view()))
				.unwrap();
		}
		PlaybackCommand::SkipTo(i) => {
			pl.skip_to(i);
			tx.send(PlaybackEvent::PlaylistUpdated(pl.playlist_view()))
				.unwrap();
		}
		PlaybackCommand::SeekForward => {
			pl.seek_forward().unwrap();
			tx.send(PlaybackEvent::PlayerUpdated(pl.player_view()))
				.unwrap();
		}
		PlaybackCommand::SeekBackward => {
			pl.seek_backward().unwrap();
			tx.send(PlaybackEvent::PlayerUpdated(pl.player_view()))
				.unwrap();
		}
		PlaybackCommand::Seek(pos) => {
			pl.seek(pos).unwrap();
			tx.send(PlaybackEvent::PlayerUpdated(pl.player_view()))
				.unwrap();
		}
	}
}

pub async fn playback_idle_tick(pl: &mut Playlist, tx: &UnboundedSender<PlaybackEvent>) {
	if pl.finished() && !pl.pause {
		let e = pl.play().await;
		if let Err(e) = e {
			eprintln!("Error occured during playback: {e}");
		}
		tx.send(PlaybackEvent::PlaylistUpdated(pl.playlist_view()))
			.unwrap();
	} else {
		tx.send(PlaybackEvent::PlayerUpdated(pl.player_view()))
			.unwrap();
	}
}

fn youtube_link(id: &str) -> String {
	format!("https://music.youtube.com/watch?v={}", id)
}

#[derive(Debug, Default, Clone)]
pub struct PlaylistView {
	pub list: Vec<TrackItem>,
	pub index: Option<usize>,
	pub player: PlayerView,
}

impl PlaylistView {}

#[derive(Debug, Default, Clone)]
pub struct PlayerView {
	pub pause: bool,
	pub pos: Duration,
	pub length: Duration,
}

impl PlayerView {
	#[allow(unused)]
	pub fn playback_time(&self) -> String {
		let played_min = self.pos.as_secs() / 60;
		let played_sec = self.pos.as_secs() % 60;
		let total_min = self.length.as_secs() / 60;
		let total_sec = self.length.as_secs() % 60;
		format!(
			"{}:{:02}/{}:{:02}",
			played_min, played_sec, total_min, total_sec
		)
	}
}
