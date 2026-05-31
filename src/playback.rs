use std::{fmt::Debug, time::Duration};

use rustypipe::model::TrackItem;
use tokio::{
	select,
	sync::mpsc::{UnboundedReceiver, UnboundedSender},
};

use crate::{
	data::update_track_view,
	player::{Player, PlayerState},
};

#[derive(Default)]
pub struct Playback {
	player: PlayerState,
	list: Vec<TrackItem>,
	index: Option<usize>,
	pause: bool,
	volume: f32,
}

impl Playback {
	pub fn new() -> Self {
		Self {
			player: PlayerState::None,
			list: Vec::new(),
			index: None,
			pause: true,
			volume: 1.0,
		}
	}

	pub async fn play(&mut self) -> anyhow::Result<()> {
		if let PlayerState::Loaded(ref player) = self.player
			&& !player.finished()
		{
			return Ok(());
		}
		if self.is_empty() {
			self.index.take();
			self.player = PlayerState::None;
			return Ok(());
		}

		let index = match self.index {
			Some(i) => (i).min(self.len() - 1),
			None => 0,
		};

		let track_item = self
			.get_track(index)
			.expect("Playlist index is greater than list size.")
			.clone();

		self.player = PlayerState::Loading(Player::new(&track_item.id, self.volume));
		self.index = Some(index);
		update_track_view(track_item);

		Ok(())
	}

	pub fn finished(&self) -> bool {
		match &self.player {
			PlayerState::Loaded(p) => p.finished(),
			PlayerState::Loading(_) => false,
			PlayerState::None => true,
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
		self.player = PlayerState::None;
		self.pause = false;
	}

	pub fn seek_forward(&mut self) -> anyhow::Result<()> {
		match &mut self.player {
			PlayerState::Loaded(p) => p.seek_forward(5.0),
			_ => Ok(()),
		}
	}

	pub fn seek_backward(&mut self) -> anyhow::Result<()> {
		match &mut self.player {
			PlayerState::Loaded(p) => p.seek_backward(5.0),
			_ => Ok(()),
		}
	}

	pub fn seek(&mut self, value: Duration) -> anyhow::Result<()> {
		match &mut self.player {
			PlayerState::Loaded(p) => p.seek(value),
			_ => Ok(()),
		}
	}

	pub fn toggle_pause(&mut self) -> anyhow::Result<()> {
		self.pause = !self.pause;
		match self.pause {
			true => {
				if let PlayerState::Loaded(ref p) = self.player {
					p.pause();
				}
			}
			false => {
				if let PlayerState::Loaded(ref p) = self.player {
					p.unpause();
				}
			}
		}

		Ok(())
	}

	pub fn skip_next(&mut self) {
		self.index = match self.index {
			None => Some(0),
			Some(x) if x == (self.len() - 1) => {
				self.pause = true;
				None
			}
			Some(x) => Some(x + 1),
		};
		self.player = PlayerState::None;
	}

	pub fn skip_prev(&mut self) {
		self.index = match self.index {
			Some(i) => i.checked_sub(1),
			None => None,
		};
		self.player = PlayerState::None;
	}

	pub fn skip_to(&mut self, index: usize) {
		self.index = Some(index);
		self.player = PlayerState::None;
	}

	pub fn set_volume(&mut self, vol: f32) {
		self.volume = vol;
		if let PlayerState::Loaded(ref p) = self.player {
			p.set_volume(self.volume);
		}
	}

	pub fn playback_view(&self) -> PlaybackView {
		PlaybackView {
			list: self.list.clone(),
			index: self.index,
			player: self.player_view(),
		}
	}

	pub fn player_view(&self) -> PlayerView {
		let pref = self.player.get_player();
		PlayerView {
			pause: self.pause,
			volume: self.volume,
			pos: pref.map(|p| p.get_pos()).unwrap_or_default(),
			length: pref.map(|p| p.duration).unwrap_or_default(),
		}
	}

	pub fn get_track(&self, index: usize) -> Option<&TrackItem> {
		self.list.get(index)
	}
}

impl Debug for Playback {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Playlist")
			.field("player", &self.player.get_player().is_some())
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
	SetVolume(f32),
}

pub enum PlaybackEvent {
	PlaylistUpdated(PlaybackView),
	PlayerUpdated(PlayerView),
}

pub async fn playback_loop(
	mut rx: UnboundedReceiver<PlaybackCommand>,
	tx: UnboundedSender<PlaybackEvent>,
) {
	let mut pl = Playback::new();
	loop {
		select! {
			Some(cmd) = rx.recv() => {
				playback_command(&mut pl, cmd, &tx).await;
			}
			_ = tokio::time::sleep(Duration::from_millis(100)) => {
				playback_idle_tick(&mut pl, &tx).await;
			}
			Some(res) = pl.player.try_finish() => {
				match res {
					Ok(p) => {
						pl.player = PlayerState::Loaded(p);
					},
					Err(e) => {
						eprintln!("Error occured during playback: {e}");
					},
				    }
			}
		}
	}
}

pub async fn playback_command(
	pl: &mut Playback,
	cmd: PlaybackCommand,
	tx: &UnboundedSender<PlaybackEvent>,
) {
	match cmd {
		PlaybackCommand::LoadPlaylist(list) => {
			pl.set_list(list);
			tx.send(PlaybackEvent::PlaylistUpdated(pl.playback_view()))
				.unwrap();
		}
		PlaybackCommand::TogglePause => {
			pl.toggle_pause().unwrap();
			tx.send(PlaybackEvent::PlayerUpdated(pl.player_view()))
				.unwrap();
		}
		PlaybackCommand::SkipNext => {
			pl.skip_next();
			tx.send(PlaybackEvent::PlaylistUpdated(pl.playback_view()))
				.unwrap();
		}
		PlaybackCommand::SkipPrev => {
			pl.skip_prev();
			tx.send(PlaybackEvent::PlaylistUpdated(pl.playback_view()))
				.unwrap();
		}
		PlaybackCommand::SkipTo(i) => {
			pl.skip_to(i);
			tx.send(PlaybackEvent::PlaylistUpdated(pl.playback_view()))
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
		PlaybackCommand::SetVolume(v) => {
			pl.set_volume(v);
			tx.send(PlaybackEvent::PlayerUpdated(pl.player_view()))
				.unwrap();
		}
	}
}

pub async fn playback_idle_tick(pl: &mut Playback, tx: &UnboundedSender<PlaybackEvent>) {
	if pl.finished() && !pl.pause {
		if pl.player.get_player().is_some() {
			pl.skip_next();
		}
		// A second check due to mutation done on skip_next
		if pl.pause {
			return;
		}
		let e = pl.play().await;
		if let Err(e) = e {
			eprintln!("Error occured during playback: {e}");
		}
		tx.send(PlaybackEvent::PlaylistUpdated(pl.playback_view()))
			.unwrap();
	} else {
		tx.send(PlaybackEvent::PlayerUpdated(pl.player_view()))
			.unwrap();
	}
}

pub fn youtube_link(id: &str) -> String {
	format!("https://music.youtube.com/watch?v={}", id)
}

#[derive(Debug, Default, Clone)]
pub struct PlaybackView {
	pub list: Vec<TrackItem>,
	pub index: Option<usize>,
	pub player: PlayerView,
}

impl PlaybackView {
	pub fn current_track(&self) -> Option<&TrackItem> {
		let i = self.index?;
		self.list.get(i)
	}
}

#[derive(Debug, Default, Clone)]
pub struct PlayerView {
	pub pause: bool,
	pub volume: f32,
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
