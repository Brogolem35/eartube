use std::time::Duration;

#[cfg(target_os = "linux")]
use iced::window::settings::PlatformSpecific;
use iced::{
	Background, Border, Color, ContentFit, Element, Length, Padding, Size, Subscription, Task,
	Theme,
	alignment::{Horizontal, Vertical},
	border::Radius,
	color, event,
	keyboard::{self, Key, key::Named},
	mouse::ScrollDelta,
	theme::{Palette, palette},
	widget::{
		Column, Row, button, column, container, image, mouse_area, row, scrollable, sensor,
		slider, space, svg, text, text_input,
	},
	window,
};
use iced_aw::{ContextMenu, Tabs, context_menu, style::colors};
use itertools::Itertools;
use rand::{rng, seq::SliceRandom};
use rustypipe::model::TrackItem;
use souvlaki::{MediaControlEvent, MediaControls};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use crate::{
	data::{self, Playlist, toggle_favorite},
	icons,
	playback::{MediaMeta, PlaybackCommand, PlaybackEvent, PlaybackView, playback_loop},
	screen,
	thumbnail::{ThumbnailCache, ThumbnailSource},
	yt::{YtSearch, new_radio, search, youtube_link, ytdlp_available},
};

use iced_reorderable::Column as Reorderable;

const SMALL_THUMBNAIL_SIZE: u32 = 80;
const SEMI_TRANSPARENT_COLOR: Color = Color::from_rgba(0.3, 0.3, 0.3, 0.3);
const MOST_VIEWED_AMOUNT: usize = 20;

struct AppState {
	ytdlp_available: bool,

	search_input: String,

	playback_view: PlaybackView,
	playback_hold_pos: Option<Duration>,

	// Favorites list is cloned everytime it is updated to avoid constant locking and lifetime issues.
	favorites_view: Vec<TrackItem>,
	most_viewed_view: Vec<TrackItem>,
	search_view: YtSearch,

	scene: Scene,
	queue_scene: bool,

	thumbnail_manager: ThumbnailCache,

	playback_tx: UnboundedSender<PlaybackCommand>,
	playback_rx: UnboundedReceiver<PlaybackEvent>,

	media_controls: Option<MediaControls>,
	media_controls_rx: UnboundedReceiver<MediaControlEvent>,
}

impl AppState {
	fn new() -> Self {
		let (player_tx, player_rx) = mpsc::unbounded_channel();
		let (event_tx, event_rx) = mpsc::unbounded_channel();

		let (mc_tx, mc_rx) = mpsc::unbounded_channel();
		let media_controls = if cfg!(target_os = "windows") {
			None
		} else {
			let mut mc = MediaControls::new(souvlaki::PlatformConfig {
				dbus_name: "eartube",
				display_name: "Eartube",
				hwnd: None,
			})
			.expect("Could not create Media Controls");
			let _ = mc.attach(move |e| {
				let _ = mc_tx.send(e);
			});
			Some(mc)
		};

		tokio::spawn(playback_loop(player_rx, event_tx));

		Self {
			ytdlp_available: ytdlp_available(),

			search_input: String::from("Bad Apple"),
			playback_view: PlaybackView::default(),
			playback_hold_pos: None,

			favorites_view: data::get_favorites().iter().cloned().rev().collect(),
			most_viewed_view: data::get_most_viewed_amount(MOST_VIEWED_AMOUNT),
			search_view: YtSearch::default(),

			scene: Scene::Home,
			queue_scene: false,

			thumbnail_manager: ThumbnailCache::default(),

			playback_tx: player_tx,
			playback_rx: event_rx,

			media_controls,
			media_controls_rx: mc_rx,
		}
	}

	fn view(&self) -> Element<'_, Message> {
		if self.ytdlp_available {
			return text(
				"yt-dlp could not be found. Make sure the executable is available on PATH and is up to date.",
			)
			.into();
		}

		let main = self.scene_boilerplate(match &self.scene {
			Scene::Home => self.view_home(),
			Scene::Search(tab) => self.view_search(*tab),
			Scene::Favorites => self.view_playlist("Favorites", &self.favorites_view),
			Scene::History(h) => self.view_playlist("History", h),
			Scene::Playlist(p) => self.view_playlist(&p.name, &p.tracks),
		});
		let queue = self.view_queue();

		screen::Screen::new(vec![(true, queue), (false, main)])
			.set_active_scene(self.queue_scene)
			.into()
	}

	fn view_home(&self) -> Element<'_, Message> {
		let favorites = self.view_home_playlist(
			"Favorites",
			&self.favorites_view,
			&self.thumbnail_manager,
		);

		let most_played = self.view_home_playlist(
			"Most Played",
			&self.most_viewed_view,
			&self.thumbnail_manager,
		);

		scrollable(column![favorites, most_played].spacing(5))
			.width(Length::Fill)
			.height(Length::Fill)
			.spacing(0)
			.id("home_scroll")
			.into()
	}

	fn view_home_playlist<'a>(
		&'a self,
		name: &'a str,
		playlist: &'a [TrackItem],
		thumb_manager: &'a ThumbnailCache,
	) -> Element<'a, Message> {
		let title = text(name).size(18).color(Color::WHITE);

		let play_icon = svg(svg::Handle::from_memory(icons::PLAY)).width(18);
		let play_text = text("Play");
		let play_row = row![play_icon, play_text]
			.align_y(Vertical::Center)
			.spacing(2);
		let play_button = button(play_row)
			.on_press_with(|| Message::StartPlaylist(playlist.to_owned()))
			.style(transparent_button_style)
			.padding(5);

		let shuffle_icon = svg(svg::Handle::from_memory(icons::SHUFFLE)).width(18);
		let shuffle_text = text("Shuffle");
		let shuffle_row = row![shuffle_icon, shuffle_text]
			.align_y(Vertical::Center)
			.spacing(2);
		let shuffle_button = button(shuffle_row)
			.on_press_with(|| Message::StartShuffle(playlist.to_owned()))
			.style(transparent_button_style)
			.padding(5);

		let upper_row = row![
			title,
			play_button,
			shuffle_button,
			space().width(Length::Fill)
		]
		.align_y(Vertical::Center)
		.spacing(10);

		let meat: Element<'_, Message> = if playlist.is_empty() {
			text("List is empty")
				.align_y(Vertical::Center)
				.align_x(Horizontal::Center)
				.height(240)
				.style(|t: &Theme| text::Style {
					color: Some(t.extended_palette().background.strong.text),
				})
				.into()
		} else {
			row(playlist.iter().map(|item| {
				let thumb = thumb_manager.get(&item.id);
				self.favorite_track_card(item, thumb)
			}))
			.spacing(2)
			.padding(Padding::new(0.0).horizontal(3))
			.into()
		};

		let pl_scroll = scrollable(meat).horizontal().spacing(5);

		column![upper_row, pl_scroll].spacing(3).into()
	}

	fn view_search(&self, active_tab: SearchTab) -> Element<'_, Message> {
		let tracks = scrollable(Column::from_iter(self.search_view.tracks.iter().map(
			|item| {
				let thumb = self.thumbnail_manager.get(&item.id);
				self.search_track_card(item, thumb)
			},
		)))
		.width(Length::Fill)
		.height(Length::Fill)
		.id("search_elements")
		.spacing(0);

		let videos = scrollable(Column::from_iter(self.search_view.videos.iter().map(
			|item| {
				let thumb = self.thumbnail_manager.get(&item.id);
				self.search_track_card(item, thumb)
			},
		)))
		.width(Length::Fill)
		.height(Length::Fill)
		.id("search_elements")
		.spacing(0);

		Tabs::new(Message::SearchTabSelected)
			.push(SearchTab::Tracks, "Tracks".into(), tracks)
			.push(SearchTab::Videos, "Videos".into(), videos)
			.set_active_tab(&active_tab)
			.into()
	}

	fn view_queue(&self) -> Element<'_, Message> {
		let playback_control = self.view_playback_control();

		let queue_elements = scrollable(
			Reorderable::from_iter(self.playback_view.queue.iter().enumerate().map(
				|(index, item)| {
					let card = self.track_card(item, index);
					(index, card)
				},
			))
			.on_drop(Message::QueueMove),
		)
		.width(Length::Fill)
		.height(Length::Fill)
		.id("queue_elements")
		.spacing(0);

		column![queue_elements, playback_control,]
			.height(Length::Fill)
			.width(Length::Fill)
			.into()
	}

	fn view_playlist<'a>(
		&'a self,
		name: &'a str,
		tracks: &'a [TrackItem],
	) -> Element<'a, Message> {
		let title = text(name).size(18).color(Color::WHITE);

		let play_icon = svg(svg::Handle::from_memory(icons::PLAY)).width(18);
		let play_text = text("Play");
		let play_row = row![play_icon, play_text]
			.align_y(Vertical::Center)
			.spacing(2);
		let play_button = button(play_row)
			.on_press_with(|| Message::StartPlaylist(tracks.to_owned()))
			.style(transparent_button_style)
			.padding(5);

		let shuffle_icon = svg(svg::Handle::from_memory(icons::SHUFFLE)).width(18);
		let shuffle_text = text("Shuffle");
		let shuffle_row = row![shuffle_icon, shuffle_text]
			.align_y(Vertical::Center)
			.spacing(2);
		let shuffle_button = button(shuffle_row)
			.on_press_with(|| Message::StartShuffle(tracks.to_owned()))
			.style(transparent_button_style)
			.padding(5);

		let upper_row = row![
			title,
			play_button,
			shuffle_button,
			space().width(Length::Fill)
		]
		.align_y(Vertical::Center)
		.spacing(10);

		let pl_scroll = scrollable(Column::from_iter(tracks.iter().map(|item| {
			let thumb = self.thumbnail_manager.get(&item.id);
			self.search_track_card(item, thumb)
		})))
		.width(Length::Fill)
		.height(Length::Fill)
		.id("playlist_elements")
		.spacing(0);

		column![upper_row, pl_scroll].spacing(3).into()
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
		let control_buttons = row![skipp_button, pause_button, skipn_button].spacing(2);

		let volume_slider = self.view_volume_slider();
		let loop_toggle = button(svg(loop_button_icon(self.playback_view.player.ploop)))
			.on_press(Message::ToggleLoop)
			.height(button_height)
			.padding(0)
			.width(button_width);
		let shuffle = button(svg(svg::Handle::from_memory(icons::SHUFFLE)))
			.on_press(Message::ShufflePlayback)
			.height(button_height)
			.padding(0)
			.width(button_width);
		let right_buttons = row![loop_toggle, shuffle];
		let right_row = row![volume_slider, right_buttons]
			.align_y(Vertical::Center)
			.spacing(50)
			.width(Length::Fill);

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

		let controls_row = row![left_row, control_buttons, right_row]
			.align_y(Vertical::Center)
			.spacing(35);

		column![playback_progress, controls_row,]
			.align_x(Horizontal::Center)
			.padding(Padding::new(10.0).top(2.0))
	}

	fn view_playback_informer(&self) -> Element<'_, Message> {
		let field = match self.playback_view.current_track() {
			Some(t) => {
				let thumb = self.thumbnail_manager.get(&t.id);
				self.controls_track_card(t, thumb)
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
			.align_y(Vertical::Center)
			.spacing(10)
			.padding(5)
	}

	fn view_volume_slider(&self) -> Row<'_, Message> {
		let vol = self.playback_view.player.volume;
		let slider = slider(0.0..=1.0, vol, Message::VolumeChanged)
			.step(0.005_f32)
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

		row![slider_area, text].align_y(Vertical::Center).spacing(5)
	}

	fn view_search_input(&self) -> Element<'_, Message> {
		let search_input = text_input("Search", &self.search_input)
			.on_input(Message::SearchEdit)
			.on_submit(Message::Search);
		let play_button = button("Search").on_press(Message::Search);

		column![search_input, play_button]
			.spacing(5)
			.padding(5)
			.into()
	}

	fn view_tabs(&self) -> Element<'_, Message> {
		column![
			button("Home")
				.on_press(Message::GoHome)
				.style(transparent_button_style)
				.width(Length::Fill),
			button("Favorites")
				.on_press(Message::GoFavorites)
				.style(transparent_button_style),
			button("History")
				.on_press(Message::GoHistory)
				.style(transparent_button_style)
				.width(Length::Fill)
		]
		.width(Length::Shrink)
		.into()
	}

	fn scene_boilerplate<'a>(&'a self, inner: Element<'a, Message>) -> Element<'a, Message> {
		let search = self.view_search_input();
		let playback_control = self.view_playback_control();

		let sep_style = |t: &Theme| container::Style {
			background: Some(t.extended_palette().background.weak.color.into()),
			..Default::default()
		};
		let vertical_sep = container("").height(Length::Fill).width(1).style(sep_style);
		let horizontal_sep1 = container("").height(1).width(Length::Fill).style(sep_style);
		let horizontal_sep2 = container("").height(1).width(Length::Fill).style(sep_style);

		let row = row![self.view_tabs(), vertical_sep, inner].spacing(1);
		column![
			search,
			horizontal_sep1,
			row,
			horizontal_sep2,
			playback_control
		]
		.into()
	}

	fn update(&mut self, message: Message) -> Task<Message> {
		match message {
			Message::Search => {
				let arg = self.search_input.clone();
				Task::perform(
					async move { search(&arg).await.map_err(|e| e.to_string()) },
					Message::FetchSearch,
				)
			}
			Message::SearchEdit(text) => {
				self.search_input = text;
				Task::none()
			}
			Message::Tick => self.tick(),
			Message::MediaControlTick => {
				let meta = MediaMeta::from(&self.playback_view);
				if let Some(ref mut mc) = self.media_controls {
					let _ = mc.set_playback(meta.playback);
				}
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
				self.playback_view.player.pause = !self.playback_view.player.pause;
				Task::none()
			}
			Message::TogglePlaylistView => {
				self.queue_scene = !self.queue_scene;
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
				self.playback_view.player.pos = pos;

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
				self.favorites_view =
					data::get_favorites().iter().cloned().rev().collect();
				Task::none()
			}
			Message::SelectTrack(track) => {
				self.playback_tx
					.send(PlaybackCommand::LoadQueue(vec![track]))
					.unwrap();
				Task::none()
			}
			Message::AddToQueue(track) => {
				self.playback_tx
					.send(PlaybackCommand::PushTrack(track))
					.unwrap();
				Task::none()
			}
			Message::RemoveFromQueue(i) => {
				self.playback_tx
					.send(PlaybackCommand::RemoveFromQueue(i))
					.unwrap();
				Task::none()
			}
			Message::StartRadio(track) => Task::perform(
				async move { new_radio(track).await.map_err(|e| e.to_string()) },
				Message::FetchQueue,
			),
			Message::StartPlaylist(items) => {
				self.playback_tx
					.send(PlaybackCommand::LoadQueue(items))
					.unwrap();
				Task::none()
			}
			Message::StartShuffle(mut items) => {
				items.shuffle(&mut rng());
				self.playback_tx
					.send(PlaybackCommand::LoadQueue(items))
					.unwrap();
				Task::none()
			}
			Message::RemoveFromHistory(id) => {
				data::remove_from_history(&id);
				self.update_history_views();
				Task::none()
			}
			Message::ToggleLoop => {
				self.playback_tx.send(PlaybackCommand::ToggleLoop).unwrap();
				Task::none()
			}
			Message::ShufflePlayback => {
				self.playback_tx.send(PlaybackCommand::Shuffle).unwrap();
				Task::none()
			}
			Message::QueueMove(from, to) => {
				self.playback_tx
					.send(PlaybackCommand::MoveTrack(from, to))
					.unwrap();
				Task::none()
			}
			Message::CopyText(s) => iced::clipboard::write(s),
			Message::GoHome => {
				self.scene = Scene::Home;
				Task::none()
			}
			Message::GoPlaylist(p) => {
				self.scene = Scene::Playlist(p);
				Task::none()
			}
			Message::GoFavorites => {
				self.scene = Scene::Favorites;
				Task::none()
			}
			Message::GoHistory => {
				self.scene = Scene::History(data::get_history());
				Task::none()
			}
			Message::SearchTabSelected(search_tab) => {
				if let Scene::Search(_) = self.scene {
					self.scene = Scene::Search(search_tab);
				}

				Task::none()
			}
			Message::FetchQueue(result) => {
				let items = match result {
					Ok(i) => i,
					Err(e) => {
						eprintln!("Error: {:?}", e);
						return Task::none();
					}
				};

				self.playback_tx
					.send(PlaybackCommand::LoadQueue(items))
					.unwrap();
				Task::none()
			}
			Message::FetchSearch(result) => {
				let items = match result {
					Ok(i) => i,
					Err(e) => {
						eprintln!("Error: {:?}", e);
						return Task::none();
					}
				};

				self.search_view = items;
				self.scene = Scene::Search(SearchTab::Tracks);
				Task::none()
			}
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
				PlaybackEvent::QueueUpdated(view) => {
					self.playback_view = view;
					self.update_history_views();
					let meta = MediaMeta::from(&self.playback_view);
					if let Some(ref mut mc) = self.media_controls {
						let _ = mc.set_metadata(meta.metadata);
					}
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

	fn update_history_views(&mut self) {
		self.most_viewed_view = data::get_most_viewed_amount(MOST_VIEWED_AMOUNT);
		if let Scene::History(_) = self.scene {
			self.scene = Scene::History(data::get_history());
		};
	}

	fn theme(&self) -> Option<Theme> {
		let palette = Palette {
			text: Color::WHITE,
			primary: color!(210, 11, 11),
			..Palette::DARK
		};
		Some(Theme::custom_with_fn(
			"Eartube",
			palette,
			Self::theme_extented,
		))
	}

	fn theme_extented(palette: Palette) -> palette::Extended {
		palette::Extended {
			background: palette::Background {
				strongest: palette::Pair::new(color!(30, 30, 30), palette.text),
				strong: palette::Pair::new(
					color!(60, 60, 60),
					color!(180, 180, 180),
				),
				..palette::Background::new(palette.background, palette.text)
			},
			primary: palette::Primary {
				weak: palette::Pair::new(color!(100, 15, 15), palette.text),
				..palette::Primary::generate(
					palette.primary,
					palette.background,
					palette.text,
				)
			},
			is_dark: true,
			..palette::Extended::generate(palette)
		}
	}

	fn is_favorited(&self, id: &str) -> bool {
		self.favorites_view.iter().any(|t| t.id == id)
	}

	#[allow(unstable_name_collisions)]
	fn track_card<'a>(&'a self, track: &'a TrackItem, index: usize) -> Element<'a, Message> {
		let msg = Message::SkipTo(index);
		let current = self
			.playback_view
			.index
			.map(|i| index == i)
			.unwrap_or(false);
		let thumb = self.thumbnail_manager.get(&track.id);

		let thumbnail = sensor(image(thumb)
			.content_fit(ContentFit::Cover)
			.height(SMALL_THUMBNAIL_SIZE)
			.width(SMALL_THUMBNAIL_SIZE))
		.on_show(|_| Message::ImagePopIn(ThumbnailSource::new(track)))
		.key_ref(&track.id);

		let name = text(&track.name).size(20);

		let artists = text(track
			.artists
			.iter()
			.map(|a| a.name.as_str())
			.intersperse(", ")
			.collect::<String>())
		.size(14)
		.style(|t: &Theme| text::Style {
			color: t.extended_palette().background.strong.text.into(),
		});

		let column = column![name, artists].spacing(6).padding(10);

		let remove_button = button(svg(svg::Handle::from_memory(icons::CROSS)))
			.height(SMALL_THUMBNAIL_SIZE)
			.width(SMALL_THUMBNAIL_SIZE)
			.style(transparent_button_style)
			.on_press(Message::RemoveFromQueue(index));

		let marea = mouse_area(
			row![
				thumbnail,
				column,
				space().width(Length::Fill),
				remove_button
			]
			.spacing(6)
			.padding(10),
		)
		.on_press(msg);

		let drag = container(svg(svg::Handle::from_memory(icons::DRAG)).width(30))
			.width(40)
			.height(Length::Fill)
			.align_x(Horizontal::Center)
			.align_y(Vertical::Center);

		container(
			row![drag, marea]
				.align_y(Vertical::Center)
				.padding(Padding::default().horizontal(10)),
		)
		.width(Length::Fill)
		.style(move |t: &Theme| track_card_style(t, current))
		.into()
	}

	#[allow(unstable_name_collisions)]
	fn favorite_track_card<'a>(
		&'a self,
		track: &'a TrackItem,
		thumb: image::Handle,
	) -> Element<'a, Message> {
		let thumbnail = sensor(image(thumb)
			.content_fit(ContentFit::Cover)
			.height(200)
			.width(200))
		.on_show(|_| Message::ImagePopIn(ThumbnailSource::new(track)))
		.key_ref(&track.id);

		let name = text(ellipsize(&track.name, 20)).size(14);

		let artists = text(ellipsize(
			&track.artists
				.iter()
				.map(|a| a.name.as_str())
				.intersperse(", ")
				.collect::<String>(),
			20,
		))
		.size(14)
		.style(|t: &Theme| text::Style {
			color: t.extended_palette().background.strong.text.into(),
		});

		let column = column![thumbnail, name, artists].spacing(6).padding(10);

		let card = container(
			button(column)
				.on_press_with(move || Message::SelectTrack(track.clone()))
				.style(transparent_button_style),
		)
		.width(Length::Fill)
		.style(|t: &Theme| track_card_style(t, false));

		self.track_context_menu(track, card.into())
	}

	#[allow(unstable_name_collisions)]
	fn controls_track_card<'a>(
		&'a self,
		track: &'a TrackItem,
		thumb: image::Handle,
	) -> Element<'a, Message> {
		let thumbnail = sensor(image(thumb)
			.content_fit(ContentFit::Cover)
			.height(SMALL_THUMBNAIL_SIZE)
			.width(SMALL_THUMBNAIL_SIZE))
		.on_show(|_| Message::ImagePopIn(ThumbnailSource::new(track)))
		.key_ref(&track.id);

		let name = text(&track.name).size(16).wrapping(text::Wrapping::None);

		let artists = text(track
			.artists
			.iter()
			.map(|a| a.name.as_str())
			.intersperse(", ")
			.collect::<String>())
		.size(14)
		.style(|t: &Theme| text::Style {
			color: t.extended_palette().background.strong.text.into(),
		})
		.wrapping(text::Wrapping::None);

		let column = column![name, artists].spacing(6).padding(10);

		container(row![thumbnail, column].spacing(6))
			.width(Length::Fill)
			.into()
	}

	#[allow(unstable_name_collisions)]
	fn search_track_card<'a>(
		&'a self,
		track: &'a TrackItem,
		thumb: image::Handle,
	) -> Element<'a, Message> {
		let thumbnail = sensor(image(thumb)
			.content_fit(ContentFit::Cover)
			.height(SMALL_THUMBNAIL_SIZE)
			.width(SMALL_THUMBNAIL_SIZE))
		.on_show(|_| Message::ImagePopIn(ThumbnailSource::new(track)))
		.key_ref(&track.id);

		let name = text(&track.name).size(20);

		let artists = text(track
			.artists
			.iter()
			.map(|a| a.name.as_str())
			.intersperse(", ")
			.collect::<String>())
		.size(14)
		.style(|t: &Theme| text::Style {
			color: t.extended_palette().background.strong.text.into(),
		});

		let column = column![name, artists].spacing(6).padding(10);
		let row = row![thumbnail, column, space().width(Length::Fill),]
			.spacing(6)
			.padding(10);

		let card = container(
			button(row)
				.on_press_with(move || Message::SelectTrack(track.clone()))
				.style(transparent_button_style),
		)
		.width(Length::Fill)
		.style(move |t: &Theme| track_card_style(t, false));

		self.track_context_menu(track, card.into())
	}

	fn track_context_menu<'a>(
		&'a self,
		track: &'a TrackItem,
		inner: Element<'a, Message>,
	) -> Element<'a, Message> {
		let fav_text = match self.is_favorited(&track.id) {
			false => "Favorite",
			true => "Unfavorite",
		};

		ContextMenu::new(inner, move || {
			column![
				button("Play")
					.on_press_with(|| Message::SelectTrack(track.clone()))
					.width(Length::Fill)
					.style(context_button_style),
				button("Add to queue")
					.on_press_with(|| Message::AddToQueue(track.clone()))
					.width(Length::Fill)
					.style(context_button_style),
				button("Start radio")
					.on_press_with(|| Message::StartRadio(track.clone()))
					.width(Length::Fill)
					.style(context_button_style),
				button(fav_text)
					.on_press_with(|| Message::ToggleFavorite(track.clone()))
					.width(Length::Fill)
					.style(context_button_style),
				button("Copy link")
					.on_press_with(|| Message::CopyText(youtube_link(
						&track.id
					)))
					.width(Length::Fill)
					.style(context_button_style),
				button("Remove from history")
					.on_press_with(|| Message::RemoveFromHistory(
						track.id.to_owned()
					))
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
}

#[derive(Debug, Clone)]
pub enum Message {
	Exit,
	Search,
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
	RemoveFromQueue(usize),
	StartRadio(TrackItem),
	StartPlaylist(Vec<TrackItem>),
	StartShuffle(Vec<TrackItem>),
	RemoveFromHistory(String),
	ToggleLoop,
	ShufflePlayback,
	CopyText(String),
	QueueMove(usize, usize),
	GoHome,
	GoPlaylist(Playlist),
	GoFavorites,
	GoHistory,
	SearchTabSelected(SearchTab),
	FetchQueue(Result<Vec<TrackItem>, String>),
	FetchSearch(Result<YtSearch, String>),
}

pub fn iced_main() -> iced::Result {
	iced::application(AppState::new, AppState::update, AppState::view)
		.settings(iced::Settings {
			id: Some(String::from("eartube")),
			..Default::default()
		})
		.window(window::Settings {
			icon: window::icon::from_file_data(icons::ICON, None).ok(),
			#[cfg(target_os = "linux")]
			platform_specific: PlatformSpecific {
				application_id: String::from("eartube"),
				..PlatformSpecific::default()
			},
			..Default::default()
		})
		.title("Eartube")
		.theme(AppState::theme)
		.exit_on_close_request(false)
		.subscription(AppState::subscription)
		.window_size(Size::new(1050.0, 870.0))
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

fn loop_button_icon(looping: bool) -> svg::Handle {
	match looping {
		true => svg::Handle::from_memory(icons::LOOP),
		false => svg::Handle::from_memory(icons::LOOP_EMPTY),
	}
}

fn duration_fmt(d: Duration) -> String {
	let d_min = d.as_secs() / 60;
	let d_sec = d.as_secs() % 60;
	format!("{}:{:02}", d_min, d_sec)
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

fn track_card_style(t: &Theme, current: bool) -> container::Style {
	match current {
		true => container::Style {
			background: Some(t.extended_palette().primary.weak.color.into()),
			border: Border {
				radius: Radius::new(12),
				width: 2.0,
				color: t.extended_palette().primary.base.color,
			},
			..Default::default()
		},
		false => container::Style {
			background: Some(t.extended_palette().background.strongest.color.into()),
			border: Border {
				radius: Radius::new(12),
				width: 1.0,
				color: t.extended_palette().background.strong.color,
			},
			..Default::default()
		},
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

#[derive(Clone, Debug)]
enum Scene {
	Home,
	Search(SearchTab),
	Favorites,
	History(Vec<TrackItem>),
	Playlist(Playlist),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchTab {
	Tracks,
	Videos,
}
