use std::time::Duration;

use iced::{
	Element, Event, Subscription, Task, event,
	widget::{Column, Row, button, column, row, text, text_input},
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
			playlist_tx: player_tx,
			playlist_rx: event_rx,
		}
	}

	fn view_playback_control(&self) -> Row<'_, Message> {
		let pause_button_icon = pause_button_icon(self.playlist_view.player.pause);
		let skipp_button = button("⏮").on_press(Message::SkipPrev);
		let seekb_button = button("⏪︎").on_press(Message::SeekBackward);
		let pause_button = button(pause_button_icon).on_press(Message::TogglePause);
		let seekf_button = button("⏩︎").on_press(Message::SeekForward);
		let skipn_button = button("⏭").on_press(Message::SkipNext);
		row![
			skipp_button,
			seekb_button,
			pause_button,
			seekf_button,
			skipn_button
		]
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

	let playback_control_row = state.view_playback_control();
	let duration_text = text(state.playlist_view.player.playback_time());

	let playlist_elements = Column::from_iter(
		state.playlist_view
			.list
			.iter()
			.map(|i| text(&i.name).into()),
	);

	column![
		search_input,
		play_button,
		duration_text,
		playback_control_row,
		playlist_elements
	]
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
