use std::time::Duration;

use iced::{
	Background, Border, Color, ContentFit, Element, Length, Subscription, Task, Theme,
	alignment::{Horizontal, Vertical},
	border::Radius,
	event,
	keyboard::{self, Key, key::Named},
	mouse::ScrollDelta,
	widget::{
		Column, Row, button, column, container, image, mouse_area, row, scrollable, sensor,
		slider, space, svg, text, text_input,
	},
	window,
};
use iced_aw::{ContextMenu, context_menu, style::colors};
use rustypipe::model::TrackItem;
use souvlaki::{MediaControlEvent, MediaControls};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::{
	data::{self, toggle_favorite},
	icons, new_radio,
	playback::{MediaMeta, PlaybackCommand, PlaybackEvent, PlaybackView, playback_loop},
	search_and_play,
	thumbnail::{ThumbnailCache, ThumbnailSource},
};

const SMALL_THUMBNAIL_SIZE: u32 = 80;
const SEMI_TRANSPARENT_COLOR: Color = Color::from_rgba(0.3, 0.3, 0.3, 0.3);

struct AppState {
	search_input: String,

	playback_view: PlaybackView,
	playback_hold_pos: Option<Duration>,

	// Favorites list is cloned everytime it is updated to avoid constant locking and lifetime issues.
	favorites_view: Vec<TrackItem>,

	view: Scene,
	playlist_scene: bool,

	thumbnail_manager: ThumbnailCache,

	playback_tx: UnboundedSender<PlaybackCommand>,
	playback_rx: UnboundedReceiver<PlaybackEvent>,

	media_controls: MediaControls,
	media_controls_rx: UnboundedReceiver<MediaControlEvent>,
}

impl AppState {
	fn new() -> Self {
		let (player_tx, player_rx) = mpsc::unbounded_channel();
		let (event_tx, event_rx) = mpsc::unbounded_channel();

		// TODO: Probably won't work on Windows. Use a real OS.
		let mut media_controls = MediaControls::new(souvlaki::PlatformConfig {
			dbus_name: "eartube",
			display_name: "Eartube",
			hwnd: None,
		})
		.unwrap();
		let (mc_tx, mc_rx) = mpsc::unbounded_channel();
		let _ = media_controls.attach(move |e| {
			let _ = mc_tx.send(e);
		});

		tokio::spawn(playback_loop(player_rx, event_tx));

		Self {
			search_input: String::from("Bad Apple"),
			playback_view: PlaybackView::default(),
			playback_hold_pos: None,

			favorites_view: data::get_favorites().clone(),

			view: Scene::MainMenu,
			playlist_scene: false,

			thumbnail_manager: ThumbnailCache::default(),

			playback_tx: player_tx,
			playback_rx: event_rx,

			media_controls,
			media_controls_rx: mc_rx,
		}
	}

	fn view(&self) -> Element<'_, Message> {
		match self.playlist_scene {
			false => match self.view {
				Scene::MainMenu => self.view_main_menu(),
			},
			true => self.view_playlist(),
		}
	}

	fn view_main_menu(&self) -> Element<'_, Message> {
		let search = self.view_search_input();
		let playback_control = self.view_playback_control();

		let fav_scroll = scrollable(row(self.favorites_view.iter().rev().map(|item| {
			let thumb = self.thumbnail_manager.get(&item.id);
			favorite_track_card(item, thumb)
		})))
		.id("fav_scroll")
		.horizontal();

		let favorites = column![text("Favorites"), fav_scroll,];

		let menu_scroll = scrollable(favorites)
			.width(Length::Fill)
			.height(Length::Fill)
			.id("main_menu_scroll");

		column![search, menu_scroll, playback_control,]
			.width(Length::Fill)
			.height(Length::Fill)
			.into()
	}

	fn view_playlist(&self) -> Element<'_, Message> {
		let playback_control = self.view_playback_control();

		let playlist_elements = scrollable(Column::from_iter(
			self.playback_view
				.list
				.iter()
				.enumerate()
				.map(|(index, item)| {
					let msg = Message::SkipTo(index);
					let current = self
						.playback_view
						.index
						.map(|i| index == i)
						.unwrap_or(false);
					let thumb = self.thumbnail_manager.get(&item.id);
					let card = track_card(item, current, thumb);
					mouse_area(card).on_press(msg).into()
				}),
		))
		.width(Length::Fill)
		.height(Length::Fill)
		.id("playlist_elements")
		.spacing(0);

		column![playlist_elements, playback_control,]
			.height(Length::Fill)
			.width(Length::Fill)
			.into()
	}

	fn view_playback_control(&self) -> Column<'_, Message> {
		let pause_button_icon = pause_button_icon(self.playback_view.player.pause);
		let button_height = Length::Fixed(30.0);
		let button_width = Length::Fixed(40.0);

		let skipp_button = button(svg(svg::Handle::from_memory(icons::PREV)))
			.on_press(Message::SkipPrev)
			.height(button_height)
			.width(button_width);
		let pause_button = button(svg(pause_button_icon))
			.on_press(Message::TogglePause)
			.height(button_height)
			.width(button_width);
		let skipn_button = button(svg(svg::Handle::from_memory(icons::NEXT)))
			.on_press(Message::SkipNext)
			.height(button_height)
			.width(button_width);

		let playback_progress = self.view_playback_progress();
		let control_buttons = row![skipp_button, pause_button, skipn_button];

		let volume_slider = self.view_volume_slider();

		let current_track = self.playback_view.current_track();
		let favorited = current_track
			.map(|t| self.is_favorited(&t.id))
			.unwrap_or(false);
		let favorite_button = button(svg(favorite_button_icon(favorited)))
			.height(button_height)
			.width(button_width)
			.on_press_maybe(current_track.map(|t| Message::ToggleFavorite(t.clone())));
		let info = self.view_playback_informer();
		let left_row = row![info, favorite_button]
			.align_y(Vertical::Center)
			.width(Length::Fill);

		let controls_row = row![left_row, control_buttons, volume_slider]
			.align_y(Vertical::Center)
			.spacing(50);

		column![playback_progress, controls_row,]
			.align_x(Horizontal::Center)
			.padding(10)
	}

	fn view_playback_informer(&self) -> Element<'_, Message> {
		let field = match self.playback_view.current_track() {
			Some(t) => {
				let thumb = self.thumbnail_manager.get(&t.id);
				controls_track_card(t, thumb)
			}
			None => space()
				.height(SMALL_THUMBNAIL_SIZE)
				.width(Length::Fill)
				.into(),
		};
		button(field)
			.padding(0)
			.style(transparent_button_style)
			.on_press(Message::TogglePlaylistView)
			.into()
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
		let vol = self.playback_view.player.volume;
		let slider = slider(0.0..=1.0, vol, Message::VolumeChanged)
			.step(0.005)
			.width(100);
		let slider_area = mouse_area(slider).on_scroll(move |d| match d {
			ScrollDelta::Lines { y, .. } | ScrollDelta::Pixels { y, .. } => {
				match y.total_cmp(&0.0) {
					std::cmp::Ordering::Less => {
						Message::VolumeChanged((vol - 0.01).max(0.0))
					}
					std::cmp::Ordering::Greater => {
						Message::VolumeChanged((vol + 0.01).min(1.0))
					}
					std::cmp::Ordering::Equal => Message::VolumeChanged(vol),
				}
			}
		});
		let vol_percent = (self.playback_view.player.volume * 100.0) as u32;
		let text = text(format!("{:>3}%", vol_percent))
			.align_x(Horizontal::Right)
			.width(40);

		row![slider_area, text]
			.align_y(Vertical::Center)
			.width(Length::Fill)
			.spacing(5)
	}

	fn view_search_input(&self) -> Element<'_, Message> {
		let search_input = text_input("Search", &self.search_input)
			.on_input(Message::SearchEdit)
			.on_submit(Message::Play);
		let play_button = button("Play").on_press(Message::Play);

		column![search_input, play_button].into()
	}

	fn update(&mut self, message: Message) -> Task<Message> {
		match message {
			Message::Play => {
				let arg = self.search_input.clone();
				Task::perform(
					async move {
						search_and_play(&arg)
							.await
							.map_err(|e| e.to_string())
					},
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

				self.playback_tx
					.send(PlaybackCommand::LoadPlaylist(items))
					.unwrap();
				Task::none()
			}
			Message::SearchEdit(text) => {
				self.search_input = text;
				Task::none()
			}
			Message::Tick => self.tick(),
			Message::MediaControlTick => {
				let meta = MediaMeta::from(&self.playback_view);
				let _ = self.media_controls.set_playback(meta.playback);
				Task::none()
			}
			Message::SeekForward => {
				self.playback_tx.send(PlaybackCommand::SeekForward).unwrap();
				Task::none()
			}
			Message::SeekBackward => {
				self.playback_tx
					.send(PlaybackCommand::SeekBackward)
					.unwrap();
				Task::none()
			}
			Message::TogglePause => {
				self.playback_tx.send(PlaybackCommand::TogglePause).unwrap();
				Task::none()
			}
			Message::TogglePlaylistView => {
				self.playlist_scene = !self.playlist_scene;
				Task::none()
			}
			Message::SkipNext => {
				self.playback_tx.send(PlaybackCommand::SkipNext).unwrap();
				Task::none()
			}
			Message::SkipPrev => {
				self.playback_tx.send(PlaybackCommand::SkipPrev).unwrap();
				Task::none()
			}
			Message::SkipTo(i) => {
				self.playback_tx.send(PlaybackCommand::SkipTo(i)).unwrap();
				Task::none()
			}
			Message::PlaybackSliderHold(pos) => {
				self.playback_hold_pos = Some(pos);
				Task::none()
			}
			Message::PlaybackSliderRelease => {
				let Some(pos) = self.playback_hold_pos.take() else {
					return Task::none();
				};
				self.playback_tx.send(PlaybackCommand::Seek(pos)).unwrap();

				Task::none()
			}
			Message::VolumeChanged(v) => {
				self.playback_tx
					.send(PlaybackCommand::SetVolume(v))
					.unwrap();
				Task::none()
			}
			Message::ImagePopIn(track) => self.thumbnail_manager.fetch(track),
			Message::ImageLoaded { id, img } => {
				self.thumbnail_manager.set(id, img);
				Task::none()
			}
			Message::ToggleFavorite(track) => {
				toggle_favorite(&track);
				self.favorites_view = data::get_favorites().to_owned();
				Task::none()
			}
			Message::SelectTrack(track) => {
				self.playback_tx
					.send(PlaybackCommand::LoadPlaylist(vec![track]))
					.unwrap();
				Task::none()
			}
			Message::AddToQueue(track) => {
				self.playback_tx
					.send(PlaybackCommand::PushTrack(track))
					.unwrap();
				Task::none()
			}
			Message::StartRadio(track) => Task::perform(
				async move { new_radio(track).await.map_err(|e| e.to_string()) },
				Message::FetchPlaylist,
			),
			Message::Exit => iced::exit(),
		}
	}

	fn subscription(&self) -> Subscription<Message> {
		Subscription::batch([
			iced::time::every(Duration::from_millis(50)).map(|_| Message::Tick),
			iced::time::every(Duration::from_secs(1))
				.map(|_| Message::MediaControlTick),
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

	fn tick(&mut self) -> Task<Message> {
		while let Ok(event) = self.playback_rx.try_recv() {
			match event {
				PlaybackEvent::PlaylistUpdated(view) => {
					self.playback_view = view;
					let meta = MediaMeta::from(&self.playback_view);
					let _ = self.media_controls.set_metadata(meta.metadata);
				}
				PlaybackEvent::PlayerUpdated(view) => {
					self.playback_view.player = view;
				}
			}
		}
		while let Ok(event) = self.media_controls_rx.try_recv() {
			match event {
				MediaControlEvent::Play => {
					self.playback_tx
						.send(PlaybackCommand::TogglePause)
						.unwrap();
				}
				MediaControlEvent::Pause => {
					self.playback_tx
						.send(PlaybackCommand::TogglePause)
						.unwrap();
				}
				MediaControlEvent::Toggle => {
					self.playback_tx
						.send(PlaybackCommand::TogglePause)
						.unwrap();
				}
				MediaControlEvent::Next => {
					self.playback_tx.send(PlaybackCommand::SkipNext).unwrap();
				}
				MediaControlEvent::Previous => {
					self.playback_tx.send(PlaybackCommand::SkipPrev).unwrap();
				}
				MediaControlEvent::Quit => return iced::exit(),
				_ => {}
			}
		}

		Task::none()
	}

	fn is_favorited(&self, id: &str) -> bool {
		self.favorites_view.iter().any(|t| t.id == id)
	}
}

#[derive(Debug, Clone)]
pub enum Message {
	Exit,
	Play,
	Tick,
	/// Has a seperate tick than the rest due to being able to process signals only once every second.
	MediaControlTick,
	TogglePause,
	TogglePlaylistView,
	SeekForward,
	SeekBackward,
	SkipNext,
	SkipPrev,
	SkipTo(usize),
	PlaybackSliderHold(Duration),
	PlaybackSliderRelease,
	VolumeChanged(f32),
	SearchEdit(String),
	ImagePopIn(ThumbnailSource),
	ImageLoaded {
		id: String,
		img: image::Handle,
	},
	ToggleFavorite(TrackItem),
	SelectTrack(TrackItem),
	AddToQueue(TrackItem),
	StartRadio(TrackItem),
	FetchPlaylist(Result<Vec<TrackItem>, String>),
}

pub fn iced_main() -> iced::Result {
	iced::application(AppState::new, AppState::update, AppState::view)
		.title("Eartube")
		.exit_on_close_request(false)
		.subscription(AppState::subscription)
		.run()
}

fn pause_button_icon(paused: bool) -> svg::Handle {
	match paused {
		true => svg::Handle::from_memory(icons::PLAY),
		false => svg::Handle::from_memory(icons::PAUSE),
	}
}

fn favorite_button_icon(favorited: bool) -> svg::Handle {
	match favorited {
		true => svg::Handle::from_memory(icons::FAVORITED),
		false => svg::Handle::from_memory(icons::FAVORITE),
	}
}

fn duration_fmt(d: Duration) -> String {
	let d_min = d.as_secs() / 60;
	let d_sec = d.as_secs() % 60;
	format!("{}:{:02}", d_min, d_sec)
}

fn track_card(track: &TrackItem, current: bool, thumb: image::Handle) -> Element<'_, Message> {
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

	let thumbnail = sensor(image(thumb)
		.content_fit(ContentFit::Cover)
		.height(SMALL_THUMBNAIL_SIZE)
		.width(SMALL_THUMBNAIL_SIZE))
	.on_show(|_| Message::ImagePopIn(ThumbnailSource::new(track)))
	.key_ref(&track.id);

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

	let column = column![name, artists].spacing(6).padding(10);

	container(row![thumbnail, column].spacing(6).padding(10))
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

fn favorite_track_card(track: &TrackItem, thumb: image::Handle) -> Element<'_, Message> {
	let (bg_color, border_color, name_color, artist_color) = {
		(
			Color::from_rgb8(30, 30, 30),
			Color::from_rgb8(60, 60, 60),
			Color::WHITE,
			Color::from_rgb8(180, 180, 180),
		)
	};
	let click_msg = Message::SelectTrack(track.clone());
	let queue_msg = Message::AddToQueue(track.clone());
	let radio_msg = Message::StartRadio(track.clone());

	let thumbnail = sensor(image(thumb)
		.content_fit(ContentFit::Cover)
		.height(200)
		.width(200))
	.on_show(|_| Message::ImagePopIn(ThumbnailSource::new(track)))
	.key_ref(&track.id);

	let name = text(ellipsize(&track.name, 20))
		.size(14)
		.style(move |_: &Theme| text::Style {
			color: Some(name_color),
		});

	let artists = text(ellipsize(
		&track.artists
			.iter()
			.map(|a| a.name.as_str())
			.collect::<Vec<_>>()
			.join(", "),
		20,
	))
	.size(14)
	.style(move |_: &Theme| text::Style {
		color: Some(artist_color),
	});

	let column = column![thumbnail, name, artists].spacing(6).padding(10);

	let card = container(column)
		.width(Length::Fill)
		.style(move |_: &Theme| container::Style {
			background: Some(Background::Color(bg_color)),
			border: Border {
				radius: Radius::new(12.0),
				width: 1.0,
				color: border_color,
			},
			..Default::default()
		});

	let click = mouse_area(card).on_press(click_msg.clone());
	ContextMenu::new(click, move || {
		column![
			button("Play")
				.on_press(click_msg.clone())
				.width(Length::Fill)
				.style(context_button_style),
			button("Add to queue")
				.on_press(queue_msg.clone())
				.style(context_button_style),
			button("Start radio")
				.on_press(radio_msg.clone())
				.width(Length::Fill)
				.style(context_button_style),
		]
		.width(Length::Shrink)
		.into()
	})
	.style(|t: &Theme, _| context_menu::Style {
		background: Background::Color(t.palette().background),
	})
	.into()
}

fn controls_track_card(track: &TrackItem, thumb: image::Handle) -> Element<'_, Message> {
	let (name_color, artist_color) = (Color::WHITE, Color::from_rgb8(180, 180, 180));

	let thumbnail = sensor(image(thumb)
		.content_fit(ContentFit::Cover)
		.height(SMALL_THUMBNAIL_SIZE)
		.width(SMALL_THUMBNAIL_SIZE))
	.on_show(|_| Message::ImagePopIn(ThumbnailSource::new(track)))
	.key_ref(&track.id);

	let name = text(&track.name)
		.size(16)
		.style(move |_: &Theme| text::Style {
			color: Some(name_color),
		})
		.wrapping(text::Wrapping::None);

	let artists = text(track
		.artists
		.iter()
		.map(|a| a.name.as_str())
		.collect::<Vec<_>>()
		.join(", "))
	.size(14)
	.style(move |_: &Theme| text::Style {
		color: Some(artist_color),
	})
	.wrapping(text::Wrapping::None);

	let column = column![name, artists].spacing(6).padding(10);

	container(row![thumbnail, column].spacing(6))
		.width(Length::Fill)
		.into()
}

fn transparent_button_style(t: &Theme, s: button::Status) -> button::Style {
	match s {
		button::Status::Hovered => button::Style {
			background: Some(Background::Color(SEMI_TRANSPARENT_COLOR)),
			text_color: t.palette().text,
			..Default::default()
		},
		_ => button::Style {
			background: Some(Background::Color(Color::TRANSPARENT)),
			text_color: t.palette().text,
			..Default::default()
		},
	}
}

fn context_button_style(t: &Theme, s: button::Status) -> button::Style {
	button::Style {
		border: Border {
			color: colors::BLACK,
			width: 0.2,
			radius: 0.0.into(),
		},
		..transparent_button_style(t, s)
	}
}

fn ellipsize(s: &str, max_chars: usize) -> String {
	if s.chars().count() <= max_chars {
		return s.to_string();
	}

	let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
	out.push('…');
	out
}

#[derive(Clone, Copy, Debug)]
enum Scene {
	MainMenu,
}
