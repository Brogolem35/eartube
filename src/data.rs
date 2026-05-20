use std::{
	fs,
	path::PathBuf,
	sync::{LazyLock, RwLock},
	time::{SystemTime, UNIX_EPOCH},
};

use dashmap::DashMap;
use foldhash::fast::FixedState;
use rustypipe::model::TrackItem;
use serde::{Deserialize, Serialize};

pub static TRACK_STATS: LazyLock<DashMap<String, TrackStat, FixedState>> =
	LazyLock::new(load_track_stats);

fn get_stats_path() -> PathBuf {
	let data_dir_name = match cfg!(debug_assertions) {
		true => "eartube-debug",
		false => "eartube",
	};
	dirs::data_local_dir()
		.expect("Unsupported OS")
		.join(data_dir_name)
		.join("track_stats.json")
}

fn load_track_stats() -> DashMap<String, TrackStat, FixedState> {
	let path = get_stats_path();
	if !path.exists() {
		eprintln!("track_stats.json does not exist, creating...");

		// Create parent directories and an empty file
		if let Some(parent) = path.parent() {
			if let Err(e) = fs::create_dir_all(parent) {
				panic!("Failed to create data directory: {e}");
			}
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

pub static PLAYLISTS: RwLock<Vec<Playlist>> = RwLock::new(Vec::new());

#[derive(Serialize, Deserialize, Clone)]
pub struct TrackStat {
	track: TrackItem,
	views: u64,
	last_viewed: UnixTime,
	favorited: bool,
}

impl TrackStat {
	pub fn new(track: TrackItem) -> Self {
		Self {
			track,
			views: 1,
			last_viewed: unix_time(),
			favorited: false,
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

pub type UnixTime = u64;

pub fn unix_time() -> UnixTime {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.expect("Don't travel back beyond the 1970")
		.as_secs()
}
