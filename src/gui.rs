use std::time::Duration;

use iced::{
	Background, Border, Color, Element, Length, Subscription, Task, Theme,
	alignment::{Horizontal, Vertical},
	border::Radius,
	event,
	keyboard::{self, Key, key::Named},
	widget::{
		Column, Row, button, column, container, mouse_area, row, scrollable, slider, space,
		text, text_input,
	},
	window,
};
use rustypipe::model::TrackItem;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::{
	playback::{PlaybackCommand, PlaybackEvent, PlaybackView, playback_loop},
	rp_testing,
};

struct AppState {
	search_input: String,
	playback_view: PlaybackView,
	playback_hold_pos: Option<Duration>,
	playback_tx: UnboundedSender<PlaybackCommand>,
	playback_rx: UnboundedReceiver<PlaybackEvent>,
}

impl AppState {
	fn new() -> Self {
		let (player_tx, player_rx) = mpsc::unbounded_channel();
		let (event_tx, event_rx) = mpsc::unbounded_channel();

		tokio::spawn(playback_loop(player_rx, event_tx));

		Self {
			search_input: String::from("Bad Apple"),
			playback_view: PlaybackView::default(),
			playback_hold_pos: None,
			playback_tx: player_tx,
			playback_rx: event_rx,
		}
	}

	fn view_playback_control(&self) -> Column<'_, Message> {
		let pause_button_icon = pause_button_icon(self.playback_view.player.pause);
		let skipp_button = button("⏮").on_press(Message::SkipPrev);
		let seekb_button = button("⏪︎").on_press(Message::SeekBackward);
		let pause_button = button(pause_button_icon).on_press(Message::TogglePause);
		let seekf_button = button("⏩︎").on_press(Message::SeekForward);
		let skipn_button = button("⏭").on_press(Message::SkipNext);

		let playback_progress = self.view_playback_progress();
		let control_buttons = row![
			skipp_button,
			seekb_button,
			pause_button,
			seekf_button,
			skipn_button
		];

		let volume_slider = self.view_volume_slider();

		// Three equal columns: spacer | buttons | slider
		// This keeps buttons visually centered regardless of slider width.
		let controls_row =
			row![space().width(Length::Fill), control_buttons, volume_slider]
				.align_y(Vertical::Center)
				.spacing(50);

		column![playback_progress, controls_row,].align_x(Horizontal::Center)
	}

	fn view_playback_progress(&self) -> Row<'_, Message> {
		let pl = &self.playback_view.player;
		let len = pl.length;
		let pos = self.playback_hold_pos.unwrap_or(pl.pos);

		let playback_slider = slider(0.0..=len.as_secs_f32(), pos.as_secs_f32(), |p| {
			Message::PlaybackSliderHold(Duration::from_secs_f32(p))
		})
		.on_release(Message::PlaybackSliderRelease);
		let playback_pos = text(duration_fmt(pos));
		let playback_len = text(duration_fmt(len));

		row![playback_pos, playback_slider, playback_len]
			.spacing(10)
			.padding(5)
	}

	fn view_volume_slider(&self) -> Row<'_, Message> {
		let slider = slider(
			0.0..=1.0,
			self.playback_view.player.volume,
			Message::VolumeChanged,
		)
		.step(0.005)
		.width(100);
		let vol_percent = (self.playback_view.player.volume * 100.0) as u32;
		let text = text(format!("{:>3}%", vol_percent))
			.align_x(Horizontal::Right)
			.width(40);

		row![slider, text]
			.align_y(Vertical::Center)
			.width(Length::Fill)
			.spacing(5)
	}
}

#[derive(Debug, Clone)]
enum Message {
	Exit,
	Play,
	Tick,
	TogglePause,
	SeekForward,
	SeekBackward,
	SkipNext,
	SkipPrev,
	SkipTo(usize),
	PlaybackSliderHold(Duration),
	PlaybackSliderRelease,
	VolumeChanged(f32),
	SearchEdit(String),
	FetchPlaylist(Result<Vec<TrackItem>, String>),
}

pub fn iced_main() -> anyhow::Result<()> {
	iced::application(AppState::new, update, view)
		.title("Eartube")
		.exit_on_close_request(false)
		.subscription(subscription)
		.run()?;
	Ok(())
}

fn view(state: &AppState) -> Element<'_, Message> {
	let search_input = text_input("Search", &state.search_input)
		.on_input(Message::SearchEdit)
		.on_submit(Message::Play);
	let play_button = button("Play").on_press(Message::Play);

	let playback_control = state.view_playback_control();

	let playlist_elements = scrollable(Column::from_iter(
		state.playback_view
			.list
			.iter()
			.enumerate()
			.map(|(index, item)| {
				let msg = Message::SkipTo(index);
				let current = state
					.playback_view
					.index
					.map(|i| index == i)
					.unwrap_or(false);
				let card = track_card(item, current);
				mouse_area(card).on_press(msg).into()
			}),
	))
	.width(Length::Fill)
	.height(Length::Fill)
	.spacing(0);

	column![
		search_input,
		play_button,
		playlist_elements,
		playback_control,
	]
	.height(Length::Fill)
	.width(Length::Fill)
	.into()
}

fn update(state: &mut AppState, message: Message) -> Task<Message> {
	match message {
		Message::Play => {
			let arg = state.search_input.clone();
			Task::perform(
				async move { rp_testing(&arg).await.map_err(|e| e.to_string()) },
				Message::FetchPlaylist,
			)
		}
		Message::FetchPlaylist(result) => {
			let items = match result {
				Ok(i) => i,
				Err(e) => {
					eprintln!("Error: {:?}", e);
					return Task::none();
				}
			};

			state.playback_tx
				.send(PlaybackCommand::LoadPlaylist(items))
				.unwrap();
			Task::none()
		}
		Message::SearchEdit(text) => {
			state.search_input = text;
			Task::none()
		}
		Message::Tick => {
			while let Ok(event) = state.playback_rx.try_recv() {
				match event {
					PlaybackEvent::PlaylistUpdated(view) => {
						state.playback_view = view;
					}
					PlaybackEvent::PlayerUpdated(view) => {
						state.playback_view.player = view;
					}
				}
			}

			Task::none()
		}
		Message::SeekForward => {
			state.playback_tx
				.send(PlaybackCommand::SeekForward)
				.unwrap();
			Task::none()
		}
		Message::SeekBackward => {
			state.playback_tx
				.send(PlaybackCommand::SeekBackward)
				.unwrap();
			Task::none()
		}
		Message::TogglePause => {
			state.playback_tx
				.send(PlaybackCommand::TogglePause)
				.unwrap();
			Task::none()
		}
		Message::SkipNext => {
			state.playback_tx.send(PlaybackCommand::SkipNext).unwrap();
			Task::none()
		}
		Message::SkipPrev => {
			state.playback_tx.send(PlaybackCommand::SkipPrev).unwrap();
			Task::none()
		}
		Message::SkipTo(i) => {
			state.playback_tx.send(PlaybackCommand::SkipTo(i)).unwrap();
			Task::none()
		}
		Message::PlaybackSliderHold(pos) => {
			state.playback_hold_pos = Some(pos);
			Task::none()
		}
		Message::PlaybackSliderRelease => {
			let Some(pos) = state.playback_hold_pos.take() else {
				return Task::none();
			};
			state.playback_tx.send(PlaybackCommand::Seek(pos)).unwrap();

			Task::none()
		}
		Message::Exit => iced::exit(),
		Message::VolumeChanged(v) => {
			state.playback_tx
				.send(PlaybackCommand::SetVolume(v))
				.unwrap();
			Task::none()
		}
	}
}

fn subscription(_state: &AppState) -> Subscription<Message> {
	Subscription::batch([
		iced::time::every(Duration::from_millis(50)).map(|_| Message::Tick),
		event::listen().filter_map(|e| match e {
			event::Event::Window(window::Event::CloseRequested) => {
				println!("Received close request. Emitting Message::Exit.");
				Some(Message::Exit)
			}
			_ => None,
		}),
		keyboard::listen().filter_map(|k| match k {
			keyboard::Event::KeyPressed { key, .. } => match key {
				Key::Named(Named::ArrowRight) => Some(Message::SeekForward),
				Key::Named(Named::ArrowLeft) => Some(Message::SeekBackward),
				Key::Named(Named::Space) => Some(Message::TogglePause),
				_ => None,
			},
			_ => None,
		}),
	])
}

fn pause_button_icon(paused: bool) -> &'static str {
	match paused {
		true => "▶",
		false => "⏸",
	}
}

fn duration_fmt(d: Duration) -> String {
	let d_min = d.as_secs() / 60;
	let d_sec = d.as_secs() % 60;
	format!("{}:{:02}", d_min, d_sec)
}

fn track_card(track: &TrackItem, current: bool) -> Element<'_, Message> {
	let (bg_color, border_color, name_color, artist_color) = if current {
		(
			Color::from_rgb8(45, 45, 60),
			Color::from_rgb8(120, 120, 255),
			Color::from_rgb8(255, 255, 255),
			Color::from_rgb8(210, 210, 255),
		)
	} else {
		(
			Color::from_rgb8(30, 30, 30),
			Color::from_rgb8(60, 60, 60),
			Color::WHITE,
			Color::from_rgb8(180, 180, 180),
		)
	};

	let name = text(&track.name)
		.size(20)
		.style(move |_: &Theme| text::Style {
			color: Some(name_color),
		});

	let artists = text(track
		.artists
		.iter()
		.map(|a| a.name.as_str())
		.collect::<Vec<_>>()
		.join(", "))
	.size(14)
	.style(move |_: &Theme| text::Style {
		color: Some(artist_color),
	});

	container(column![name, artists].spacing(6).padding(14))
		.width(Length::Fill)
		.style(move |_: &Theme| container::Style {
			background: Some(Background::Color(bg_color)),
			border: Border {
				radius: Radius::new(12.0),
				width: if current { 2.0 } else { 1.0 },
				color: border_color,
			},
			..Default::default()
		})
		.into()
}
