use std::time::Duration;

use iced::{
	Element, Event, Length, Subscription, Task,
	alignment::Horizontal,
	event,
	widget::{
		Column, Row, button, column, mouse_area, row, scrollable, slider, text, text_input,
	},
	window,
};
use rustypipe::model::TrackItem;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::{
	playlist::{PlaybackCommand, PlaybackEvent, PlaylistView, playback_loop},
	rp_testing,
};

struct AppState {
	search_input: String,
	playlist_view: PlaylistView,
	playback_hold_pos: Option<Duration>,
	playlist_tx: UnboundedSender<PlaybackCommand>,
	playlist_rx: UnboundedReceiver<PlaybackEvent>,
}

impl AppState {
	fn new() -> Self {
		let (player_tx, player_rx) = mpsc::unbounded_channel();
		let (event_tx, event_rx) = mpsc::unbounded_channel();

		tokio::spawn(playback_loop(player_rx, event_tx));

		Self {
			search_input: String::from("Bad Apple"),
			playlist_view: PlaylistView::default(),
			playback_hold_pos: None,
			playlist_tx: player_tx,
			playlist_rx: event_rx,
		}
	}

	fn view_playback_control(&self) -> Column<'_, Message> {
		let pause_button_icon = pause_button_icon(self.playlist_view.player.pause);
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

		column![playback_progress, control_buttons,].align_x(Horizontal::Center)
	}

	fn view_playback_progress(&self) -> Row<'_, Message> {
		let pl = &self.playlist_view.player;
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
		state.playlist_view
			.list
			.iter()
			.enumerate()
			.map(|(index, item)| {
				let msg = Message::SkipTo(index);
				mouse_area(text(&item.name)).on_press(msg).into()
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

			state.playlist_tx
				.send(PlaybackCommand::LoadPlaylist(items))
				.unwrap();
			Task::none()
		}
		Message::SearchEdit(text) => {
			state.search_input = text;
			Task::none()
		}
		Message::Tick => {
			while let Ok(event) = state.playlist_rx.try_recv() {
				match event {
					PlaybackEvent::PlaylistUpdated(view) => {
						state.playlist_view = view;
					}
					PlaybackEvent::PlayerUpdated(view) => {
						state.playlist_view.player = view;
					}
				}
			}

			Task::none()
		}
		Message::SeekForward => {
			state.playlist_tx
				.send(PlaybackCommand::SeekForward)
				.unwrap();
			Task::none()
		}
		Message::SeekBackward => {
			state.playlist_tx
				.send(PlaybackCommand::SeekBackward)
				.unwrap();
			Task::none()
		}
		Message::TogglePause => {
			state.playlist_tx
				.send(PlaybackCommand::TogglePause)
				.unwrap();
			Task::none()
		}
		Message::SkipNext => {
			state.playlist_tx.send(PlaybackCommand::SkipNext).unwrap();
			Task::none()
		}
		Message::SkipPrev => {
			state.playlist_tx.send(PlaybackCommand::SkipPrev).unwrap();
			Task::none()
		}
		Message::SkipTo(i) => {
			state.playlist_tx.send(PlaybackCommand::SkipTo(i)).unwrap();
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
			state.playlist_tx.send(PlaybackCommand::Seek(pos)).unwrap();

			Task::none()
		}
		Message::Exit => iced::exit(),
	}
}

fn subscription(_state: &AppState) -> Subscription<Message> {
	Subscription::batch([
		event::listen().filter_map(|e| match e {
			Event::Window(window::Event::CloseRequested) => {
				println!("Received close request. Emitting Message::Exit.");
				Some(Message::Exit)
			}
			_ => None,
		}),
		iced::time::every(Duration::from_millis(50)).map(|_| Message::Tick),
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
