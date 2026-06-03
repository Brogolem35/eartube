use std::{
	cmp::Reverse,
	fs,
	path::PathBuf,
	sync::LazyLock,
	time::{SystemTime, UNIX_EPOCH},
};

use dashmap::DashMap;
use foldhash::fast::FixedState;
use iter_tools::Itertools;
use parking_lot::{RwLock, RwLockReadGuard};
use rustypipe::model::TrackItem;
use serde::{Deserialize, Serialize};

use crate::audio_cache;

static TRACK_STATS: LazyLock<DashMap<String, TrackStat, FixedState>> =
	LazyLock::new(load_track_stats);

fn get_stats_path() -> PathBuf {
	data_dir().join("track_stats.json")
}

fn load_track_stats() -> DashMap<String, TrackStat, FixedState> {
	let path = get_stats_path();
	if !path.exists() {
		eprintln!("track_stats.json does not exist, creating...");

		// Create parent directories and an empty file
		if let Some(parent) = path.parent()
			&& let Err(e) = fs::create_dir_all(parent)
		{
			panic!("Failed to create data directory: {e}");
		}
		if let Err(e) = fs::write(&path, "{}") {
			panic!("Failed to create track_stats.json: {e}");
		}
		return DashMap::default();
	}

	let content = match fs::read_to_string(&path) {
		Ok(contents) => contents,
		Err(e) => panic!("Failed to read track stats file: {e}"),
	};

	let res = match serde_json::from_str(&content) {
		Ok(res) => res,
		Err(e) => panic!("Failed to read track stats file: {e}"),
	};

	let updated_content = serde_json::to_string_pretty(&res).unwrap();
	if updated_content != content {
		eprintln!(
			"track_stats.json does not match its deserialized counterpart, updating..."
		);
		save_track_stats_content(updated_content);
	}

	res
}

pub fn save_track_stats() {
	match serde_json::to_string_pretty(&*TRACK_STATS) {
		Ok(json) => save_track_stats_content(json),
		Err(e) => panic!("Failed to serialize track stats: {e}"),
	}
}

fn save_track_stats_content(content: String) {
	let path = get_stats_path();
	if let Err(e) = fs::write(&path, content) {
		eprintln!("Failed to save track stats: {e}");
	}
}

pub fn update_track_view(track_item: TrackItem) {
	match TRACK_STATS.entry(track_item.id.clone()) {
		dashmap::Entry::Occupied(mut view) => view.get_mut().add_view(),
		dashmap::Entry::Vacant(view) => {
			view.insert(TrackStat::new(track_item));
		}
	}

	save_track_stats();
}

pub fn get_most_viewed_amount(amount: usize) -> Vec<TrackItem> {
	TRACK_STATS
		.iter()
		.map(|i| i.value().clone())
		.sorted_unstable_by_key(|v| (Reverse(v.views), Reverse(v.last_viewed)))
		.take(amount)
		.map(|v| v.track)
		.collect()
}

static FAVORITES: LazyLock<RwLock<Vec<TrackItem>>> =
	LazyLock::new(|| RwLock::new(load_favorites()));

fn get_favorites_path() -> PathBuf {
	data_dir().join("favorites.json")
}

fn load_favorites() -> Vec<TrackItem> {
	let path = get_favorites_path();
	if !path.exists() {
		eprintln!("favorites.json does not exist, creating...");

		// Create parent directories and an empty file
		if let Some(parent) = path.parent()
			&& let Err(e) = fs::create_dir_all(parent)
		{
			panic!("Failed to create data directory: {e}");
		}
		if let Err(e) = fs::write(&path, "[]") {
			panic!("Failed to create favorites.json: {e}");
		}
		return Vec::new();
	}

	let content = match fs::read_to_string(&path) {
		Ok(contents) => contents,
		Err(e) => panic!("Failed to read favorites.json: {e}"),
	};

	let res = match serde_json::from_str(&content) {
		Ok(res) => res,
		Err(e) => panic!("Failed to read favorites.json: {e}"),
	};

	let updated_content = serde_json::to_string_pretty(&res).unwrap();
	if updated_content != content {
		eprintln!(
			"favorites.json does not match its deserialized counterpart, updating..."
		);
		save_favorites_content(updated_content);
	}

	res
}

pub fn save_favorites() {
	match serde_json::to_string_pretty(&*FAVORITES.read()) {
		Ok(json) => save_favorites_content(json),
		Err(e) => panic!("Failed to serialize favorites.json: {e}"),
	}
}

fn save_favorites_content(content: String) {
	let path = get_favorites_path();
	if let Err(e) = fs::write(&path, content) {
		eprintln!("Failed to save favorites.json: {e}");
	}
}

pub fn is_favorited(id: &str) -> bool {
	FAVORITES.read().iter().any(|t| t.id == id)
}

pub fn toggle_favorite(track: &TrackItem) {
	{
		let mut lock = FAVORITES.write();
		let id = track.id.as_str();
		match lock.iter().position(|t| t.id == id) {
			Some(i) => {
				lock.remove(i);
			}
			None => {
				audio_cache::fetch(id);
				lock.push(track.clone());
			}
		};
	}

	save_favorites();
}

pub fn get_favorites() -> RwLockReadGuard<'static, Vec<TrackItem>> {
	FAVORITES.read()
}

pub static PLAYLISTS: RwLock<Vec<Playlist>> = RwLock::new(Vec::new());

#[derive(Serialize, Deserialize, Clone)]
pub struct TrackStat {
	track: TrackItem,
	views: u64,
	last_viewed: UnixTime,
}

impl TrackStat {
	pub fn new(track: TrackItem) -> Self {
		Self {
			track,
			views: 1,
			last_viewed: unix_time(),
		}
	}

	pub fn add_view(&mut self) {
		self.views = self.views.saturating_add(1);
		self.last_viewed = unix_time();
	}
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Playlist {
	name: String,
	tracks: Vec<TrackItem>,
}

pub fn data_dir() -> PathBuf {
	let name = match cfg!(debug_assertions) {
		true => "eartube-debug",
		false => "eartube",
	};
	dirs::data_local_dir().expect("Unsupported OS").join(name)
}

pub fn cache_dir() -> PathBuf {
	data_dir().join("cache")
}

pub fn img_cache_dir() -> PathBuf {
	cache_dir().join("img")
}

pub fn audio_cache_dir() -> PathBuf {
	cache_dir().join("audio")
}

pub type UnixTime = u64;

pub fn unix_time() -> UnixTime {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.expect("Don't travel back beyond the 1970")
		.as_secs()
}
